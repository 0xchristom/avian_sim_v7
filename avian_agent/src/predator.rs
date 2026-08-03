//! Predator entity + fleeing (2.2).
//!
//! The predator is a **bespoke pursuit script, NOT a BTNode** — a single
//! always-on chase does not need the full Selector/Sequence tree. 4.1 ships
//! flight: fleeing pigeons fly away at `FLY_SPEED_MS`, and the predator's
//! speed (`PREDATOR_SPEED_MS`) is calibrated between a sick pigeon's half-speed
//! flight (7.5) and a healthy pigeon's full flight (15) so healthy pigeons can
//! genuinely escape while sick/surprised pigeons are run down.
//!
//! Flow per tick (called from `run_systems`):
//! 1. `collect_threats` — detection pass BEFORE the agent loop: which agents
//!    are within `PREDATOR_DETECTION_RADIUS_M` AND inside their FOV. Feeds the
//!    2.0 `Flee` branch (highest priority) + `Alarm` telemetry.
//! 2. `plan_movement` — chase the nearest agent, applied before `physics.step`.
//! 3. `resolve_contact` — after positions sync: dist < `PREDATOR_CONTACT_DISTANCE_M`
//!    (1.0m, ≈ combined body radii; a 0.3m gap is unreachable while the 0.4+0.5
//!    colliders overlap-free) → 50% kill / 50% miss+cooldown.

use avian_core::calibration;
use avian_core::components::*;
use avian_core::rng::SimRng;
use avian_core::Simulation;
use hecs::Entity;
use nalgebra::Vector2;
use rustc_hash::FxHashMap;

/// Detection pass: for each agent, find the nearest predator within detection
/// radius AND inside the agent's FOV cone. Returns agent → flee direction
/// (away from the predator). Agents with no visible predator are absent.
pub fn collect_threats(sim: &Simulation) -> FxHashMap<Entity, Vector2<f64>> {
    let mut threats = FxHashMap::default();
    let predators: Vec<Vector2<f64>> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(_, (p, _))| p.0)
        .collect();
    if predators.is_empty() {
        return threats;
    }

    for (id, (pos, head, vision, _meta, fsm)) in sim
        .world
        .query::<(&Position, &Heading, &Vision, &Metabolism, &FSMState)>()
        .iter()
    {
        let mut nearest: Option<(f64, Vector2<f64>)> = None;
        // 4.4: rain shrinks the predator-detection range like every other
        // vision path.
        let detect_radius = calibration::PREDATOR_DETECTION_RADIUS_M
            * calibration::weather_vision_scale(
                sim.environment.weather,
                sim.environment.weather_intensity,
            );
        for ppos in &predators {
            let offset = *ppos - pos.0;
            let dist = offset.norm();
            if dist > detect_radius || dist < 1e-6 {
                continue;
            }
            // FOV cone check (mirrors perception::cone_cast). An already-
            // fleeing pigeon does NOT lose the threat by turning its back:
            // the hawk sits ~180° behind, outside the 170° half-cone, which
            // otherwise cancels Flee one frame after it starts (the "1-frame
            // flee" bug that made Fleeing invisible in the viewer). Commit to
            // the escape until the hawk leaves detection radius.
            if *fsm != FSMState::Fleeing {
                let angle = offset.y.atan2(offset.x) - head.0;
                // `rem_euclid` (NOT `%`): Rust's float `%` keeps the dividend's
                // sign, so `(angle + π) % 2π` yields values outside [-π, π]
                // whenever `angle < -π` (heading in the left half-plane) and
                // the FOV gate then rejects predators that are actually right
                // in front of the pigeon — the "13/30 alarm" detection hole.
                let norm_ang = crate::perception::normalize_angle_relative(angle);
                if norm_ang.abs() > vision.fov_degrees.to_radians() / 2.0 {
                    continue;
                }
            }
            // 4.3: a wall/building on the sight line hides the hawk.
            if sim
                .physics
                .cast_ray_to_static(pos.0, offset, 1.0)
                .map_or(false, |toi| toi < 1.0 - calibration::LOS_BLOCK_EPS)
            {
                continue;
            }
            let away = -offset / dist;
            if nearest.map_or(true, |(d, _)| dist < d) {
                nearest = Some((dist, away));
            }
        }
        if let Some((_, dir)) = nearest {
            threats.insert(id, dir);
        }
    }
    threats
}

/// Pursuit plan: velocity for each predator (6.2 hunt-state machine).
///
/// Speed is dynamic on the 1 (slow)..5 (very fast) scale:
/// - **Await** — no prey inside the detection radius → slow patrol (speed
///   level decays toward 1) toward the ranging waypoint.
/// - **Chase** — an agent is inside the radius (and no reposition cooldown) →
///   pursue the nearest at a speed level that RAMPS toward 5 (very fast).
/// - **Catch** — just struck (capture or miss): halted "busy" for
///   `PREDATOR_CATCH_BUSY_S` (1 s), then back to Await.
///
/// Applied to physics bodies BEFORE `physics.step` so the predator integrates
/// with the same solver step as everyone else.
pub fn plan_movement(
    sim: &mut Simulation,
    positions: &FxHashMap<Entity, Vector2<f64>>,
) -> Vec<(Entity, Vector2<f64>)> {
    let dt = sim.config.dt;
    let max_level = calibration::PREDATOR_SPEED_LEVEL_MAX as f64;
    let min_level = calibration::PREDATOR_SPEED_LEVEL_MIN as f64;

    // Snapshot predator state so we can touch `sim.rng` while planning.
    let predators_data: Vec<(Entity, Vector2<f64>, Predator)> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(id, (p, pr))| (id, p.0, *pr))
        .collect();

    // Read phase: compute linvels + refreshed patrol targets.
    let mut updates: Vec<(Entity, Vector2<f64>, Predator)> = Vec::new();
    for (id, ppos, mut pred) in predators_data {
        // 6.2 Catch beat: the predator is busy (halted) while the timer runs.
        if pred.hunt_state == PredatorHuntState::Catch {
            pred.hunt_timer_s -= dt;
            if pred.hunt_timer_s <= 0.0 {
                pred.hunt_timer_s = 0.0;
                pred.hunt_state = PredatorHuntState::Await;
            }
            updates.push((id, Vector2::zeros(), pred));
            continue;
        }

        // Sprint 1 (Audit 5): candidates must have a stable tie-break BEFORE
        // any RNG draw. Find the nearest agent, tie-breaking on entity id so
        // the choice is collection-order independent and identical across runs.
        let mut nearest: Option<(f64, Vector2<f64>, u64)> = None;
        for (aid, apos) in positions.iter() {
            let d = (*apos - ppos).norm();
            let bits = aid.to_bits().get();
            let better = match nearest {
                None => true,
                Some((b, _, bbits)) => d < b - 1e-12 || ((d - b).abs() < 1e-12 && bits < bbits),
            };
            if better {
                nearest = Some((d, *apos, bits));
            }
        }

        let mut new_patrol = pred.patrol_target;
        // During a reposition cooldown the predator is flying AWAY to a fresh
        // area — patrol toward the waypoint instead of re-engaging.
        let chasing = pred.capture_cooldown == 0;
        let linvel = match nearest {
            Some((d, target, _)) if chasing && d <= calibration::PREDATOR_DETECTION_RADIUS_M => {
                // 6.2 Chase: ramp speed toward very-fast (5).
                pred.hunt_state = PredatorHuntState::Chase;
                let next =
                    (pred.speed_level as f64) + calibration::PREDATOR_SPEED_RAMP_LEVELS_PER_S * dt;
                pred.speed_level = next.min(max_level).round().max(min_level) as u8;
                let speed = speed_for_level(pred.speed_level);
                let dir = target - ppos;
                if dir.norm() > 1e-6 {
                    dir / dir.norm() * speed
                } else {
                    Vector2::zeros()
                }
            }
            _ => {
                // 6.2 Await: patrol, speed decays toward slow (1).
                pred.hunt_state = PredatorHuntState::Await;
                let next =
                    (pred.speed_level as f64) - calibration::PREDATOR_SPEED_DECAY_LEVELS_PER_S * dt;
                pred.speed_level = next.max(min_level).round().min(max_level) as u8;
                let speed = speed_for_level(pred.speed_level);
                // Patrol mode: waypoint, re-randomize once reached. 4.3: sample
                // obstacle-free points so the hawk never patrols INTO a building.
                let target = pred.patrol_target.unwrap_or_else(|| {
                    random_patrol_target(
                        &mut sim.rng,
                        sim.config.world_width,
                        sim.config.world_height,
                        &sim.obstacles,
                    )
                });
                let dir = target - ppos;
                if dir.norm() < 1.0 {
                    new_patrol = Some(random_patrol_target(
                        &mut sim.rng,
                        sim.config.world_width,
                        sim.config.world_height,
                        &sim.obstacles,
                    ));
                }
                if dir.norm() > 1e-6 {
                    dir / dir.norm() * speed
                } else {
                    Vector2::zeros()
                }
            }
        };
        // Sprint 1 (Audit 5): persist the refreshed patrol waypoint — the old
        // code computed `new_patrol` but never wrote it back into `pred`, so a
        // predator that reached its waypoint kept patroling the SAME spot
        // forever instead of ranging the map.
        pred.patrol_target = new_patrol;
        updates.push((id, linvel, pred));
    }

    // Write phase: persist refreshed patrol targets + hunt state.
    for (id, _, pred) in &updates {
        if let Ok(mut cur) = sim.world.get::<&mut Predator>(*id) {
            cur.patrol_target = pred.patrol_target;
            cur.hunt_state = pred.hunt_state;
            cur.speed_level = pred.speed_level;
            cur.hunt_timer_s = pred.hunt_timer_s;
        }
    }

    updates.into_iter().map(|(id, v, _)| (id, v)).collect()
}

/// 6.2: absolute speed for a speed level — linear from `PREDATOR_SPEED_MS/5`
/// (slow, level 1) up to `PREDATOR_SPEED_MS` (very fast, level 5).
fn speed_for_level(level: u8) -> f64 {
    calibration::PREDATOR_SPEED_MS * (level as f64) / calibration::PREDATOR_SPEED_LEVEL_MAX as f64
}

fn random_patrol_target(
    rng: &mut SimRng,
    w: f64,
    h: f64,
    obstacles: &[avian_core::components::Obstacle],
) -> Vector2<f64> {
    // Sprint 2 (Audit 5): `random_free_point` now returns `Option`; an
    // obstacle-covered arena falls back to the world center so the predator
    // keeps a valid (non-collider) waypoint.
    avian_core::Simulation::random_free_point(w, h, obstacles, rng)
        .unwrap_or_else(|| Vector2::new(w / 2.0, h / 2.0))
}

/// A patrol waypoint at least `PREDATOR_REPOSITION_MIN_DIST` away from `pos`,
/// so a hawk that just struck sweeps into a NEW area instead of re-pinning the
/// same cluster.
fn far_random_point(
    rng: &mut SimRng,
    pos: Vector2<f64>,
    w: f64,
    h: f64,
    obstacles: &[avian_core::components::Obstacle],
) -> Vector2<f64> {
    for _ in 0..16 {
        let p = random_patrol_target(rng, w, h, obstacles);
        if (p - pos).norm() >= calibration::PREDATOR_REPOSITION_MIN_DIST_M {
            return p;
        }
    }
    random_patrol_target(rng, w, h, obstacles)
}

/// Contact resolution: after position sync, roll capture for each predator/
/// agent pair within contact distance. Kills despawn the agent; misses give
/// the predator a capture cooldown. Also decrements all predator cooldowns.
/// Returns the stable UIDs of captured agents (3.2 reward attribution).
pub fn resolve_contact(
    sim: &mut Simulation,
    positions: &FxHashMap<Entity, Vector2<f64>>,
) -> Vec<String> {
    let preds: Vec<(Entity, Vector2<f64>, u32)> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(id, (p, pr))| (id, p.0, pr.capture_cooldown))
        .collect();

    let mut kills: Vec<Entity> = Vec::new();
    // Sprint 1 (Audit 5): iterate contact candidates in a stable entity-id
    // order so the RNG draw order (and therefore which agent a contact roll is
    // made against) never depends on hash-map collection order.
    let mut contact_order: Vec<(Entity, Vector2<f64>)> =
        positions.iter().map(|(aid, apos)| (*aid, *apos)).collect();
    contact_order.sort_by_key(|(aid, _)| aid.to_bits().get());
    for (pid, ppos, cooldown) in &preds {
        if *cooldown > 0 {
            continue;
        }
        for (aid, apos) in &contact_order {
            if kills.contains(aid) {
                continue;
            }
            if (*apos - ppos).norm() < calibration::PREDATOR_CONTACT_DISTANCE_M {
                let roll: f64 = sim.rng.gen();
                if roll < calibration::PREDATOR_CAPTURE_PROBABILITY {
                    kills.push(*aid);
                    // Reposition after a strike: fly to a far waypoint and
                    // stop chasing while repositioning, so the predator RANGES
                    // the map instead of pinning one local cluster.
                    if let Ok(mut pr) = sim.world.get::<&mut Predator>(*pid) {
                        pr.patrol_target = Some(far_random_point(
                            &mut sim.rng,
                            *ppos,
                            sim.config.world_width,
                            sim.config.world_height,
                            &sim.obstacles,
                        ));
                        pr.capture_cooldown = calibration::PREDATOR_REPOSITION_COOLDOWN_FRAMES;
                        // 6.2: count the meal + enter the 1 s "busy" Catch beat.
                        pr.meals_eaten += 1;
                        pr.hunt_state = PredatorHuntState::Catch;
                        pr.hunt_timer_s = calibration::PREDATOR_CATCH_BUSY_S;
                    }
                } else if let Ok(mut pr) = sim.world.get::<&mut Predator>(*pid) {
                    pr.capture_cooldown = calibration::PREDATOR_MISS_COOLDOWN_FRAMES;
                    // 6.2: a miss also puts the hawk briefly out of action.
                    pr.hunt_state = PredatorHuntState::Catch;
                    pr.hunt_timer_s = calibration::PREDATOR_CATCH_BUSY_S;
                }
            }
        }
    }

    let mut captured_uids: Vec<String> = Vec::new();
    for aid in &kills {
        if let Ok(uid) = sim.world.get::<&AgentUid>(*aid) {
            captured_uids.push(uid.0.clone());
        }
        let handle = sim.world.get::<&PhysicsHandle>(*aid).ok().map(|h| h.0);
        // 7.2: energy removed from the live pool when the agent is captured.
        if let Ok(meta) = sim.world.get::<&Metabolism>(*aid) {
            sim.total_energy_lost_at_death_kj += meta.energy_kj;
        }
        sim.world.despawn(*aid).ok();
        if let Some(h) = handle {
            sim.physics.remove_body(h);
        }
        sim.deaths += 1;
        sim.predator_kills += 1;
    }

    // Decrement cooldowns each frame.
    for (id, _, _) in &preds {
        if let Ok(mut pr) = sim.world.get::<&mut Predator>(*id) {
            pr.capture_cooldown = pr.capture_cooldown.saturating_sub(1);
        }
    }

    // 6.2: a predator that reached its meal quota despawns satisfied ("eats
    // N pigeons, then disappears"). Logged as RemovePredator so the
    // disappearance is a ground-truth event, same as lifetime expiry.
    if sim.config.predator_fill_meals {
        let quota = sim.config.predator_fill_meals_target;
        let satiated: Vec<(Entity, String)> = sim
            .world
            .query::<(&Predator, &AgentUid)>()
            .iter()
            .filter(|(_, (pr, _))| pr.meals_eaten >= quota)
            .map(|(id, (_, uid))| (id, uid.0.clone()))
            .collect();
        if !satiated.is_empty() {
            let frame = sim.time.frame;
            for (id, uid) in satiated {
                let handle = sim.world.get::<&PhysicsHandle>(id).ok().map(|h| h.0);
                sim.world.despawn(id).ok();
                if let Some(h) = handle {
                    sim.physics.remove_body(h);
                }
                sim.events_log.push((
                    frame,
                    avian_core::events::Event::RemovePredator(
                        avian_core::events::RemovePredatorRequest { uid },
                    ),
                    avian_core::events::EventOutcome::Applied,
                ));
            }
        }
    }

    captured_uids
}

/// 2.2b: count each predator's lifetime down by `dt`. Returns the entities
/// whose lifetime elapsed — the caller despawns them (physics body + entity)
/// and logs `RemovePredator` so the disappearance is a ground-truth event.
pub fn tick_lifetimes(sim: &mut Simulation, dt: f64) -> Vec<(Entity, String)> {
    let mut expired = Vec::new();
    for (id, (pred, uid)) in sim.world.query_mut::<(&mut Predator, &AgentUid)>() {
        pred.lifetime_remaining_s -= dt;
        if pred.lifetime_remaining_s <= 0.0 {
            expired.push((id, uid.0.clone()));
        }
    }
    expired
}
