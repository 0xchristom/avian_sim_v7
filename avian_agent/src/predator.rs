//! Predator entity + fleeing (2.2).
//!
//! The predator is a **bespoke pursuit script, NOT a BTNode** — a single
//! always-on chase does not need the full Selector/Sequence tree. Flight vs
//! ground sprint is explicit: v1 fleeing is a "ground sprint away at
//! max_speed_ms", pending real flight in 4.1. It is NOT calibrated ground
//! truth; the v2 (post-4.1) acceptance criterion uses flight speed.
//!
//! Flow per tick (called from `run_systems`):
//! 1. `collect_threats` — detection pass BEFORE the agent loop: which agents
//!    are within `PREDATOR_DETECTION_RADIUS_M` AND inside their FOV. Feeds the
//!    2.0 `Flee` branch (highest priority) + `Alarm` telemetry.
//! 2. `plan_movement` — chase the nearest agent, applied before `physics.step`.
//! 3. `resolve_contact` — after positions sync: dist < `PREDATOR_CONTACT_DISTANCE_M`
//!    (1.0m, ≈ combined body radii; a 0.3m gap is unreachable while the 0.4+0.5
//!    colliders overlap-free) → 50% kill / 50% miss+cooldown.

use avian_core::Simulation;
use avian_core::calibration;
use avian_core::components::*;
use avian_core::rng::SimRng;
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

    for (id, (pos, head, vision, _meta)) in
        sim.world.query::<(&Position, &Heading, &Vision, &Metabolism)>().iter()
    {
        let mut nearest: Option<(f64, Vector2<f64>)> = None;
        for ppos in &predators {
            let offset = *ppos - pos.0;
            let dist = offset.norm();
            if dist > calibration::PREDATOR_DETECTION_RADIUS_M || dist < 1e-6 {
                continue;
            }
            // FOV cone check (mirrors perception::cone_cast).
            let angle = offset.y.atan2(offset.x) - head.0;
            let norm_ang =
                ((angle + std::f64::consts::PI) % (2.0 * std::f64::consts::PI)) - std::f64::consts::PI;
            if norm_ang.abs() > vision.fov_degrees.to_radians() / 2.0 {
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

/// Pursuit plan: velocity for each predator.
///
/// - If an agent is inside the predator's detection radius → chase the nearest.
/// - Otherwise → patrol toward a waypoint (re-randomized when reached), so the
///   predator RANGES across the map instead of pinning one cluster.
///
/// Applied to physics bodies BEFORE `physics.step` so the predator integrates
/// with the same solver step as everyone else.
pub fn plan_movement(sim: &mut Simulation, positions: &FxHashMap<Entity, Vector2<f64>>) -> Vec<(Entity, Vector2<f64>)> {
    let pred_speed = calibration::PREDATOR_SPEED_MULTIPLIER * calibration::WALK_SPEED_MS;

    // Snapshot predator state so we can touch `sim.rng` while planning.
    let predators_data: Vec<(Entity, Vector2<f64>, u32, Option<Vector2<f64>>)> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(id, (p, pr))| (id, p.0, pr.capture_cooldown, pr.patrol_target))
        .collect();

    // Read phase: compute linvels + refreshed patrol targets.
    let mut updates: Vec<(Entity, Vector2<f64>, Option<Vector2<f64>>)> = Vec::new();
    for (id, ppos, cooldown, patrol_target) in predators_data {
        let mut nearest: Option<(f64, Vector2<f64>)> = None;
        for (_aid, apos) in positions.iter() {
            let d = (*apos - ppos).norm();
            if nearest.map_or(true, |(b, _)| d < b) {
                nearest = Some((d, *apos));
            }
        }

        let mut new_patrol = patrol_target;
        // During a reposition cooldown the predator is flying AWAY to a fresh
        // area — patrol toward the waypoint instead of re-engaging.
        let chasing = cooldown == 0;
        let linvel = match nearest {
            Some((d, target)) if chasing && d <= calibration::PREDATOR_DETECTION_RADIUS_M => {
                let dir = target - ppos;
                if dir.norm() > 1e-6 {
                    dir / dir.norm() * pred_speed
                } else {
                    Vector2::zeros()
                }
            }
            _ => {
                // Patrol mode: waypoint, re-randomize once reached.
                let target = patrol_target.unwrap_or_else(|| random_patrol_target(&mut sim.rng));
                let dir = target - ppos;
                if dir.norm() < 1.0 {
                    new_patrol = Some(random_patrol_target(&mut sim.rng));
                }
                if dir.norm() > 1e-6 {
                    dir / dir.norm() * pred_speed
                } else {
                    Vector2::zeros()
                }
            }
        };
        updates.push((id, linvel, new_patrol));
    }

    // Write phase: persist refreshed patrol targets.
    for (id, _, target) in &updates {
        if let Some(t) = target {
            if let Ok(mut pred) = sim.world.get::<&mut Predator>(*id) {
                pred.patrol_target = Some(*t);
            }
        }
    }

    updates.into_iter().map(|(id, v, _)| (id, v)).collect()
}

fn random_patrol_target(rng: &mut SimRng) -> Vector2<f64> {
    let x = rng.gen_range(2.0..30.0);
    let y = rng.gen_range(2.0..19.0);
    Vector2::new(x, y)
}

/// A patrol waypoint at least `PREDATOR_REPOSITION_MIN_DIST` away from `pos`,
/// so a hawk that just struck sweeps into a NEW area instead of re-pinning the
/// same cluster.
fn far_random_point(rng: &mut SimRng, pos: Vector2<f64>) -> Vector2<f64> {
    for _ in 0..16 {
        let p = random_patrol_target(rng);
        if (p - pos).norm() >= calibration::PREDATOR_REPOSITION_MIN_DIST_M {
            return p;
        }
    }
    random_patrol_target(rng)
}

/// Contact resolution: after position sync, roll capture for each predator/
/// agent pair within contact distance. Kills despawn the agent; misses give
/// the predator a capture cooldown. Also decrements all predator cooldowns.
/// Returns the stable UIDs of captured agents (3.2 reward attribution).
pub fn resolve_contact(sim: &mut Simulation, positions: &FxHashMap<Entity, Vector2<f64>>) -> Vec<String> {
    let preds: Vec<(Entity, Vector2<f64>, u32)> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(id, (p, pr))| (id, p.0, pr.capture_cooldown))
        .collect();

    let mut kills: Vec<Entity> = Vec::new();
    for (pid, ppos, cooldown) in &preds {
        if *cooldown > 0 {
            continue;
        }
        for (aid, apos) in positions.iter() {
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
                        pr.patrol_target = Some(far_random_point(&mut sim.rng, *ppos));
                        pr.capture_cooldown = calibration::PREDATOR_REPOSITION_COOLDOWN_FRAMES;
                    }
                } else if let Ok(mut pr) = sim.world.get::<&mut Predator>(*pid) {
                    pr.capture_cooldown = calibration::PREDATOR_MISS_COOLDOWN_FRAMES;
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
