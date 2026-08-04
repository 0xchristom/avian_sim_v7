use crate::behavior_tree::{build_default_tree, AgentContext};
use crate::flocking;
use crate::gerontology::{mortality_hazard, spawn_agent};
use crate::locomotion::HeadBobSystem;
use crate::metabolism::metabolism_system;
use crate::perception::cone_cast;
use crate::predator;
use avian_core::calibration;
use avian_core::components::*;
use avian_core::Simulation;
use avian_core::{GrainVisCacheEntry, NeighborCacheEntry};
use hecs::Entity;
use nalgebra::Vector2;
use rustc_hash::FxHashMap;
use std::collections::HashSet;

pub fn spawn_grain(sim: &mut Simulation, pos: Vector2<f64>, amount: u32) -> Entity {
    sim.spawn_grain_entity(pos, amount)
}

/// 4.2 spatial memory: upsert a food location at strength 1.0 with a fresh
/// TTL. If the slot already holds food near the same spot, refresh it (no
/// duplicate); else append, LRU-evicting the lowest-strength slot at the cap.
fn remember_food(slots: &mut Vec<MemorySlot>, pos: Vector2<f64>) {
    let near = slots
        .iter_mut()
        .find(|s| (s.pos - pos).norm() < calibration::MEMORY_FOUND_DIST_M);
    if let Some(slot) = near {
        slot.pos = pos;
        slot.strength = 1.0;
        slot.ttl_frames = calibration::MEMORY_DECAY_FRAMES;
        return;
    }
    if slots.len() >= calibration::MEMORY_SLOTS_MAX {
        if let Some(idx) = slots
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.strength
                    .partial_cmp(&b.1.strength)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
        {
            slots.remove(idx);
        }
    }
    slots.push(MemorySlot {
        pos,
        strength: 1.0,
        ttl_frames: calibration::MEMORY_DECAY_FRAMES,
    });
}

pub fn run_systems(sim: &mut Simulation, dt: f64) {
    // 2.3 day/night cycle. A full day = sim.config.day_length_sim_s sim-seconds
    // (Audit 4 §9.7: scenario-tunable via simulation.toml; defaults to the
    // calibration constant DAY_LENGTH_SIM_S for headless runs).
    sim.environment.time_of_day_hours += dt / sim.config.day_length_sim_s * 24.0;
    sim.environment.time_of_day_hours %= 24.0;
    let h = sim.environment.time_of_day_hours;
    // Smooth sinusoid: noon (12h) = 1.0, midnight (0h/24h) = 0.1.
    sim.environment.light_level =
        0.1 + 0.9 * (0.5 + 0.5 * (2.0 * std::f64::consts::PI * (h - 12.0) / 24.0).cos());
    // Phase 9 (Audit 3): sun direction drives which building side hosts the
    // thermal updraft. Sunrise (6h) = east (0), noon (12h) = south (-π/2),
    // sunset (18h) = west (π). Deterministic function of time only.
    sim.environment.sun_heading = -(h - 6.0) / 12.0 * std::f64::consts::PI;
    sim.update_thermals();

    // 4.4: stochastic weather scheduler (config-gated; no-op when disabled).
    crate::weather::update(sim);

    // 2.5: drain the bounded injected-event journal. The events themselves are
    // game state (viewer event log + scenario replay); the journal is re-filled
    // during this tick and read back by the caller/tests after it returns.
    sim.events_log.clear();

    // 4.4: current weather multipliers, shared by every agent this frame so
    // the two drain paths (metabolism_system + inline mirror) stay in lockstep.
    let env_weather = sim.environment.weather;
    let env_intensity = sim.environment.weather_intensity;
    let heat_mult = calibration::weather_metabolic_multiplier(env_weather, env_intensity);
    let wind_flight_mult = calibration::weather_wind_flight_multiplier(env_weather, env_intensity);
    let vis_scale = calibration::weather_vision_scale(env_weather, env_intensity);
    let vision_range = calibration::VISION_MAX_RANGE_M * vis_scale;

    // 7.2: account metabolism_system's drain + digestion inflow for the
    // energy-balance test.
    let (drained, digested) = metabolism_system(
        &mut sim.world,
        &sim.time,
        sim.environment.light_level,
        heat_mult,
        wind_flight_mult,
    );
    sim.total_energy_expenditure_kj += drained;
    sim.total_energy_intake_kj += digested;

    // Spatial grid rebuild (once per tick). Agents only — grains excluded so
    // they never appear as neighbors. Velocities map feeds boids alignment.
    // Sprint 2 (Audit 5, B21): a single `(&Position, &Velocity, &Metabolism)`
    // query replaces the scattered per-entity `World::get` calls, so the agent
    // population is visited exactly once per tick.
    // Sprint 2 (Audit 5, B22): INCREMENTAL update — only agents that crossed a
    // cell boundary are re-bucketed (no full clear+reinsert), and entities that
    // despawned since the last tick are dropped via `sync_from`. Unmoved agents
    // keep their bucket slots, so a mostly-stationary population costs ~0
    // re-hashes per frame.
    sim.spatial_grid.last_update_moves = 0;
    let mut positions = FxHashMap::default();
    let mut velocities = FxHashMap::default();
    for (id, (pos, vel, _meta)) in sim
        .world
        .query::<(&Position, &Velocity, &Metabolism)>()
        .iter()
    {
        positions.insert(id, pos.0);
        velocities.insert(id, vel.0);
        sim.spatial_grid.update(id, pos.0);
    }
    sim.spatial_grid.sync_from(&positions);

    // Audit 3 (Phase 2): grains get their own spatial index (rebuilt per tick,
    // capacity retained) so visibility + consumption are O(agents × local
    // grains) instead of O(agents × grains). `grain_info` maps entity → (pos,
    // amount); `grain_order` records the world-query order so consumption can
    // pick grains in the SAME order as the legacy full-scan loop (bit-identical
    // behavior, deterministic).
    sim.grain_grid.clear();
    let mut grain_info: FxHashMap<Entity, (Vector2<f64>, u32)> = FxHashMap::default();
    let mut grain_order: FxHashMap<Entity, usize> = FxHashMap::default();
    let mut gi = 0usize;
    for (id, (pos, grain)) in sim.world.query::<(&Position, &Grain)>().iter() {
        if grain.amount > 0 {
            sim.grain_grid.insert(id, pos.0);
            grain_info.insert(id, (pos.0, grain.amount));
            grain_order.insert(id, gi);
            gi += 1;
        }
    }

    // Audit 3 (Phase 2): prune stale phase-2 cache entries for despawned
    // entities every CACHE_PRUNE_FRAMES frames. Bounded and deterministic —
    // hecs entity ids are generation-aware, so a stale key can never alias a
    // reused slot, but it would still grow without bound over a long run.
    if sim
        .time
        .frame
        .is_multiple_of(calibration::CACHE_PRUNE_FRAMES)
    {
        let live: HashSet<Entity> = sim
            .world
            .query::<&Metabolism>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        sim.grain_vis_cache.retain(|k, _| live.contains(k));
        sim.neighbor_cache.retain(|k, _| live.contains(k));
    }

    // 2.2 detection pass: agent → flee direction for any visible predator.
    let threats = predator::collect_threats(sim);

    // 4.2 spatial memory: decay remembered slots and pick each agent's
    // memory-biased forage target (weighted by strength) BEFORE the main loop,
    // so the tree tick can consume it without bloating the 14-element query.
    let mut memory_targets: FxHashMap<Entity, Option<Vector2<f64>>> = FxHashMap::default();
    for (id, (memory, _)) in sim.world.query_mut::<(&mut MemorySlots, &Position)>() {
        memory.slots.retain_mut(|slot| {
            slot.ttl_frames = slot.ttl_frames.saturating_sub(1);
            slot.strength = slot.ttl_frames as f64 / calibration::MEMORY_DECAY_FRAMES as f64;
            slot.strength >= calibration::MEMORY_MIN_STRENGTH
        });
        let total: f64 = memory.slots.iter().map(|s| s.strength).sum();
        let target = if total > 0.0 {
            // Pick the strongest remembered location (weighted by memory
            // strength; deterministic — no RNG draw, so the shared stream is
            // not perturbed by the memory pre-pass). Tie-break: most recent.
            memory
                .slots
                .iter()
                .max_by(|a, b| {
                    a.strength
                        .partial_cmp(&b.strength)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.ttl_frames.cmp(&b.ttl_frames))
                })
                .map(|s| s.pos)
        } else {
            None
        };
        memory_targets.insert(id, target);
    }

    let tree = build_default_tree();
    let mut commands: Vec<(Entity, Vector2<f64>)> = Vec::new();
    let mut to_despawn: Vec<Entity> = Vec::new();
    // Audit 3 (Phase 2): with no static obstacles the ONLY fixed colliders are
    // the four boundary walls. Any interior→interior sight segment lies inside
    // the convex arena and can never hit them, so LOS raycasts are pure waste —
    // skip them entirely (bit-identical result, since they can never occlude).
    let has_obstacles = !sim.obstacles.is_empty();

    for (
        id,
        (
            pos,
            head,
            vel,
            meta,
            levy,
            fsm,
            mass,
            mobility,
            vision,
            head_bob,
            age,
            _phys_handle,
            alarm,
            feather,
        ),
    ) in sim.world.query_mut::<(
        &mut Position,
        &mut Heading,
        &mut Velocity,
        &mut Metabolism,
        &mut LevyState,
        &mut FSMState,
        &Mass,
        &Mobility,
        &Vision,
        &mut HeadBob,
        &mut Age,
        &PhysicsHandle,
        &mut Alarm,
        &mut FeatherCondition,
    )>() {
        // 2.6 feather decay — rain multiplies the rate (hook for 4.4).
        // Gate on daylight: a roosting (sleeping) bird's feathers do NOT
        // degrade. Decaying unconditionally would let every bird clamp at 0
        // overnight and then preen together at dawn — synchronized preening
        // that survives the per-agent spawn desync. Halt decay below the
        // roost threshold so each bird keeps its phase across the night.
        // 2.6 feather decay — rain multiplies the rate (hook for 4.4).
        // Gate on daylight: a roosting (sleeping) bird's feathers do NOT
        // degrade. Decaying unconditionally would let every bird clamp at 0
        // overnight and then preen together at dawn — synchronized preening
        // that survives the per-agent spawn desync. Halt decay below the
        // roost threshold so each bird keeps its phase across the night.
        if sim.environment.light_level >= calibration::NIGHT_REST_LIGHT_THRESHOLD {
            let rain_mult = if sim.environment.weather == Weather::Rain {
                calibration::RAIN_FEATHER_DECAY_MULTIPLIER
            } else {
                1.0
            };
            feather.0 = (feather.0 - calibration::FEATHER_DECAY_RATE_S * dt * rain_mult).max(0.0);
        }

        // Fix #4: Age progression — 1 sim second = 1 real second of aging.
        age.years += dt / (365.0 * 24.0 * 3600.0); // seconds to years
        age.months = ((age.years * 12.0) % 12.0) as u8;
        // 4.0: vitality from the single calibrated decay model.
        age.vitality = calibration::vitality_at(age.years);

        // 2.4 death checks: starvation, old age, Gompertz-Makeham hazard.
        let dt_years = dt / (365.0 * 24.0 * 3600.0);
        let death_prob = mortality_hazard(age.years) * dt_years;
        let roll: f64 = sim.rng.gen();
        let starved = meta.energy_kj <= 0.0;
        let old_age = age.vitality < 0.001;
        if starved || old_age || roll < death_prob {
            // 6.2: record age at death for the survival-curve histogram.
            sim.death_ages.push(age.years);
            to_despawn.push(id);
            continue;
        }

        // Energy drain (inline mirror of metabolism_system, with night factor).
        // 4.1: same FLIGHT_MR_MULTIPLIER helper as metabolism_system so the
        // 7.2 energy-balance accounting stays exact across the two drains.
        // 4.4: heat scales BMR; wind scales the flight MR when airborne.
        let mass_kg = mass.current_g / 1000.0;
        let v_mag = vel.0.norm();
        let flying = v_mag >= calibration::FLIGHT_SPEED_THRESHOLD_MS;
        let wind_on_flight = if flying { wind_flight_mult } else { 1.0 };
        // Phase 9 (Audit 3): a Gliding bird's MR collapses to
        // GLIDE_MR_MULTIPLIER and its cost-of-transport term is ZERO — the
        // updraft supplies both lift and forward motion, so soaring is near-
        // costless. Same helper as metabolism_system keeps the 7.2
        // energy-balance accounting exact across the two drains.
        let gliding = *fsm == FSMState::Gliding;
        let bmr_kj_s = meta.bmr_watts
            * calibration::flight_mr_multiplier_state(v_mag, gliding)
            * heat_mult
            * wind_on_flight
            / 1000.0;
        let cot_kj_s = if gliding {
            0.0
        } else {
            12.5 * mass_kg * v_mag / 1000.0
        };
        let night_factor = if sim.environment.light_level < calibration::NIGHT_REST_LIGHT_THRESHOLD
        {
            calibration::NIGHT_DRAIN_FACTOR
        } else {
            1.0
        };
        let drain = (bmr_kj_s + cot_kj_s) * dt * night_factor;
        let actual_drain = drain.min(meta.energy_kj);
        meta.energy_kj -= actual_drain;
        // 7.2: account the amount ACTUALLY drained (clamped at zero) for the
        // energy-balance test.
        sim.total_energy_expenditure_kj += actual_drain;

        let blood_glucose = meta.gizzard_count as f64 * 0.5;
        meta.hunger = 0.6 * (1.0 - meta.crop_count as f64 / meta.crop_max as f64)
            + 0.4 * (1.0 - blood_glucose / 5.0).max(0.0);

        // Fix #6: Query neighbors only from agent positions (grains excluded).
        // 4.4: vision_range shrinks in rain (wet feathers, overcast).
        // Audit 3 (Phase 2): memoize the neighbor SET for dense, stable flocks
        // (refreshed every NEIGHBOR_REFRESH_FRAMES); unstable or sparse agents
        // refresh every frame. Distances are always recomputed from fresh
        // positions, so the steering force stays smooth and the throttle is
        // deterministic (frame-based, no wall-clock input).
        let cached_nb = sim.neighbor_cache.get(&id);
        let stable = cached_nb.is_some_and(|c| {
            c.last_count >= calibration::NEIGHBOR_STABLE_MIN_COUNT
                && (vel.0 - c.last_vel).norm() <= calibration::NEIGHBOR_STABLE_VEL_EPS
        });
        let refresh_period: u32 = if stable {
            calibration::NEIGHBOR_REFRESH_FRAMES
        } else {
            1
        };
        let neighbors_raw: Vec<(Entity, f64)> = if sim.time.frame.is_multiple_of(refresh_period) {
            let raw = sim
                .spatial_grid
                .query_k_nearest(pos.0, 7, vision_range, &positions);
            sim.neighbor_cache.insert(
                id,
                NeighborCacheEntry {
                    neighbors: raw.iter().map(|(e, _)| *e).collect(),
                    last_count: raw.len(),
                    last_vel: vel.0,
                },
            );
            raw
        } else {
            sim.neighbor_cache
                .get(&id)
                .map(|c| {
                    c.neighbors
                        .iter()
                        .filter_map(|e| positions.get(e).map(|p| (*e, (p - pos.0).norm())))
                        .collect()
                })
                .unwrap_or_else(|| {
                    sim.spatial_grid
                        .query_k_nearest(pos.0, 7, vision_range, &positions)
                })
        };
        let targets: Vec<(Entity, Vector2<f64>)> = neighbors_raw
            .iter()
            .filter_map(|(e, _)| positions.get(e).map(|p| (*e, *p)))
            .collect();
        // 4.3: line-of-sight occlusion — walls/buildings block neighbor AND
        // grain vision. `sim.physics` is a disjoint field from `sim.world`
        // (borrowed by query_mut above), so raycasting here is sound.
        let physics = &sim.physics;
        let occluded = |target: &Vector2<f64>, _dist: f64| -> bool {
            if !has_obstacles {
                return false;
            }
            physics
                .cast_ray_to_static(pos.0, *target - pos.0, 1.0)
                .is_some_and(|toi| toi < 1.0 - calibration::LOS_BLOCK_EPS)
        };
        let visible_neighbors = cone_cast(
            pos.0,
            head.0,
            vision.fov_degrees,
            vision_range,
            &targets,
            occluded,
        );
        let visible_neighbor_entities: Vec<Entity> =
            visible_neighbors.iter().map(|(e, _, _)| *e).collect();

        // Audit 3 (Phase 2): cached visible-grain list. Reuse the previous
        // tick's list while the agent hasn't moved/rotated beyond tolerance and
        // the grain set is unchanged; otherwise recompute from the grain
        // spatial index (cone + LOS raycast only over local candidates).
        let cache_fresh = sim.grain_vis_cache.get(&id).is_some_and(|c| {
            (c.pos - pos.0).norm() <= calibration::GRAIN_VIS_CACHE_POS_EPS
                && (c.heading - head.0)
                    .abs()
                    .min(std::f64::consts::TAU - (c.heading - head.0).abs())
                    <= calibration::GRAIN_VIS_CACHE_ANGLE_EPS
                && (c.vision_range - vision_range).abs() <= calibration::GRAIN_VIS_CACHE_RANGE_EPS
                && c.grains_version == sim.grains_version
        });
        let cached_visible = if cache_fresh {
            sim.grain_vis_cache.get(&id).map(|c| &c.visible)
        } else {
            None
        };
        let visible_grains: Vec<(Entity, Vector2<f64>, u32)> = if let Some(v) = cached_visible {
            v.clone()
        } else {
            let mut candidates = Vec::new();
            sim.grain_grid
                .query_radius_into(pos.0, vision_range, &mut candidates, &mut |e| {
                    grain_info.get(&e).map(|(p, _)| *p)
                });
            let visible: Vec<(Entity, Vector2<f64>, u32)> = candidates
                .iter()
                .filter_map(|e| {
                    let (g_pos, g_amt) = *grain_info.get(e)?;
                    let dir = g_pos - pos.0;
                    let dist = dir.norm();
                    if dist > vision_range || dist < 1e-6 {
                        return None;
                    }
                    let angle = dir.y.atan2(dir.x) - head.0;
                    let norm_ang = crate::perception::normalize_angle_relative(angle);
                    if norm_ang.abs() > vision.fov_degrees.to_radians() / 2.0 {
                        return None;
                    }
                    // 4.3: hide grain behind a wall/building even inside the FOV cone.
                    if has_obstacles
                        && physics
                            .cast_ray_to_static(pos.0, g_pos - pos.0, 1.0)
                            .is_some_and(|toi| toi < 1.0 - calibration::LOS_BLOCK_EPS)
                    {
                        return None;
                    }
                    Some((*e, g_pos, g_amt))
                })
                .collect();
            sim.grain_vis_cache.insert(
                id,
                GrainVisCacheEntry {
                    pos: pos.0,
                    heading: head.0,
                    vision_range,
                    grains_version: sim.grains_version,
                    visible: visible.clone(),
                },
            );
            visible
        };

        let fleeing = threats.contains_key(&id);
        let flee_dir = threats.get(&id).copied().unwrap_or(Vector2::zeros());
        alarm.0 = fleeing;
        let sick = age.vitality < calibration::SICK_VITALITY_THRESHOLD;

        let mut ctx = AgentContext {
            pos,
            head,
            vel,
            meta,
            fsm,
            levy,
            mass,
            mobility,
            vision,
            head_bob,
            neighbors: visible_neighbor_entities.clone(),
            grains: visible_grains,
            rng: &mut sim.rng,
            dt,
            light_level: sim.environment.light_level,
            feathers: feather,
            fleeing,
            flee_dir,
            sick,
            memory_target: memory_targets.get(&id).copied().flatten(),
            // 5.2: scenario-tunable hunger threshold for the root Forage
            // condition (defaults to the biology constant).
            forage_hunger_threshold: sim.config.foraging_threshold,
            // Phase 9 (Audit 3): building-thermal updraft zones for Gliding.
            thermals: &sim.thermals,
            // Audit 5a (Sprint 3): arena size for boundary avoidance.
            world_dims: Vector2::new(sim.config.world_width, sim.config.world_height),
        };

        let _ = tree.tick(&mut ctx);

        // 2.1 boids-as-force: sum steering onto the tree-selected velocity.
        // Suppressed while fleeing (a fleeing pigeon does not align with its
        // flock), while preening (a preening pigeon stands still), while
        // roosting (a sleeping bird does not align/cohere), and when the
        // 5.2 scenario disables flocking (config.flocking_enabled).
        if sim.config.flocking_enabled
            && *ctx.fsm != FSMState::Fleeing
            && *ctx.fsm != FSMState::Preening
            && *ctx.fsm != FSMState::Roosting
        {
            // Audit 5a (Sprint 2): steer only from neighbors within the
            // calibrated local radius. The neighbor CACHE may still be the
            // 10 m vision-range k-nearest set (it also feeds LOS cone-casting),
            // but the boids steering input must be radius-filtered — otherwise
            // the cohesion field is >3× larger than documented and every bird
            // within 10 m gets gravitationally captured.
            let boid_neighbors: Vec<(Vector2<f64>, Vector2<f64>, f64)> = neighbors_raw
                .iter()
                .filter(|(_, d)| *d <= calibration::BOID_NEIGHBOR_RADIUS_M)
                .filter_map(|(e, d)| match (positions.get(e), velocities.get(e)) {
                    (Some(p), Some(v)) if *d > 1e-6 => Some((*p, *v, *d)),
                    _ => None,
                })
                .collect();
            let weights = flocking::weights_for_state(*ctx.fsm, &flocking::default_weights());
            let steer = flocking::steering(ctx.pos.0, &boid_neighbors, &weights);
            // Phase 9 (Audit 3): gliding restricts steering agility — the bird
            // rides the updraft in a straight-ish line and cannot bank into the
            // flock, so the boids maneuvering force is scaled way down.
            let steer = if *ctx.fsm == FSMState::Gliding {
                steer * calibration::GLIDE_STEERING_MULTIPLIER
            } else {
                steer
            };
            ctx.vel.0 += steer;
        }

        // Audit 5a (Sprint 3): boundary-avoidance steering. A CRW/Lévy wanderer
        // holds its heading until the step burns out; against a wall the physics
        // yields tangential wall-sliding, so it used to cling to the edge in a
        // straight line forever. This soft repulsion pushes back toward the
        // interior, scaled by proximity (strongest AT the wall, zero outside the
        // margin). Applied for every non-fleeing/non-gliding state — a bird can
        // still touch an edge occasionally, but never keeps sliding along it.
        if *ctx.fsm != FSMState::Fleeing && *ctx.fsm != FSMState::Gliding {
            let margin = calibration::WALL_AVOID_MARGIN_M;
            let mut repel = Vector2::zeros();
            let x = ctx.pos.0.x;
            let y = ctx.pos.0.y;
            let (w, h) = (ctx.world_dims.x, ctx.world_dims.y);
            if x < margin {
                repel.x += (1.0 - x / margin).min(1.0);
            } else if x > w - margin {
                repel.x -= (1.0 - (w - x) / margin).min(1.0);
            }
            if y < margin {
                repel.y += (1.0 - y / margin).min(1.0);
            } else if y > h - margin {
                repel.y -= (1.0 - (h - y) / margin).min(1.0);
            }
            ctx.vel.0 += repel * calibration::WALL_AVOID_STRENGTH;
        }

        // 2.7: sick agents move at SICK_SPEED_MULTIPLIER (incl. fleeing → more
        // vulnerable to predators).
        if ctx.sick {
            ctx.vel.0 *= calibration::SICK_SPEED_MULTIPLIER;
        }

        // Audit 5a (Sprint 2): clamp the ground speed to the pigeon's own
        // max_speed_ms. Boids steering is a FORCE and was never clamped, so
        // cohesion at 5 m injected ~2.5 m/s onto a wanderer whose own walk speed
        // is 0.96 m/s — steering routinely exceeded max_speed_ms and kept birds
        // glued to the flock. Fleeing and Gliding are airborne (the tree sets
        // FLY_SPEED_MS / GLIDE_SPEED_MS above max_speed_ms) and are exempt; the
        // sick multiplier above already reduced us.
        if *ctx.fsm != FSMState::Fleeing && *ctx.fsm != FSMState::Gliding {
            let speed = ctx.vel.0.norm();
            if speed > ctx.mobility.max_speed_ms {
                ctx.vel.0 = ctx.vel.0 / speed * ctx.mobility.max_speed_ms;
            }
        }

        let mut head_bob_system = HeadBobSystem {
            time_in_phase: ctx.head_bob.time_in_phase,
            hold_duration: ctx.head_bob.hold_duration,
            thrust_duration: ctx.head_bob.thrust_duration,
            current_phase: ctx.head_bob.phase,
        };
        let (phase, offset) = head_bob_system.update(ctx.vel, ctx.head.0, dt);
        ctx.head_bob.phase = phase;
        ctx.head_bob.offset = offset;
        ctx.head_bob.time_in_phase = head_bob_system.time_in_phase;

        commands.push((id, ctx.vel.0));
    }

    // 2.6: alarm-prev roll-forward (the flee-success reward tracking was
    // removed with the RLHF export shell; AlarmPrev still advances so its
    // checkpoint roundtrip stays exercised).
    for (_, (alarm_prev, alarm)) in sim.world.query_mut::<(&mut AlarmPrev, &Alarm)>() {
        alarm_prev.0 = alarm.0;
    }

    // Apply agent velocities + predator pursuit, then one physics step.
    // 4.4: while Wind is active, a global drift is ADDED to every body (agents
    // AND predators), scaled by the smooth weather intensity.
    let wind_drift: Vector2<f64> = if env_weather == Weather::Wind {
        let i = env_intensity;
        Vector2::new(
            calibration::WIND_SPEED_MS * i * sim.environment.wind_heading.cos(),
            calibration::WIND_SPEED_MS * i * sim.environment.wind_heading.sin(),
        )
    } else {
        Vector2::zeros()
    };
    for (id, linvel) in commands {
        if let Ok(handle) = sim.world.get::<&PhysicsHandle>(id) {
            let v = linvel + wind_drift;
            // Sprint 2 (B10): don't force-wake sleeping/idle bodies when their
            // requested velocity hasn't materially changed.
            sim.physics.set_linvel_if_changed(
                handle.0,
                nalgebra::Vector2::new(v.x as f32, v.y as f32),
                calibration::BODY_VELOCITY_WAKE_EPS,
            );
        }
    }
    let pred_moves = predator::plan_movement(sim, &positions);
    for (id, linvel) in pred_moves {
        if let Ok(handle) = sim.world.get::<&PhysicsHandle>(id) {
            let v = linvel + wind_drift;
            sim.physics.set_linvel_if_changed(
                handle.0,
                nalgebra::Vector2::new(v.x as f32, v.y as f32),
                calibration::BODY_VELOCITY_WAKE_EPS,
            );
        }
    }

    sim.physics.step();

    // Sync agent positions from physics + grain consumption.
    let mut grains_to_consume: Vec<Entity> = Vec::new();
    let mut consumed_set: HashSet<Entity> = HashSet::new();

    for (_id, (pos, _head, phys_handle, meta, fsm, memory)) in sim.world.query_mut::<(
        &mut Position,
        &mut Heading,
        &PhysicsHandle,
        &mut Metabolism,
        &FSMState,
        &mut MemorySlots,
    )>() {
        if let Some(rb) = sim.physics.get_body(phys_handle.0) {
            let rb_pos = rb.translation();
            pos.0.x = rb_pos.x as f64;
            pos.0.y = rb_pos.y as f64;
        }

        if *fsm == FSMState::Foraging {
            // Audit 3 (Phase 2): query only local grains via the grain spatial
            // index instead of scanning the whole set, then sort candidates by
            // world-query order so the picked grain matches the legacy full-scan
            // loop exactly (bit-identical trajectories, deterministic).
            let mut candidates: Vec<Entity> = Vec::new();
            sim.grain_grid
                .query_radius_into(pos.0, 0.5, &mut candidates, &mut |e| {
                    grain_info.get(&e).map(|(p, _)| *p)
                });
            candidates.sort_by_key(|e| grain_order.get(e).copied().unwrap_or(usize::MAX));
            for g_id in candidates {
                if consumed_set.contains(&g_id) {
                    continue;
                }
                if let Some((g_pos, _)) = grain_info.get(&g_id) {
                    if (g_pos - pos.0).norm() >= 0.5 {
                        continue;
                    }
                    grains_to_consume.push(g_id);
                    consumed_set.insert(g_id);
                    meta.crop_count = (meta.crop_count + 1).min(meta.crop_max);
                    meta.energy_kj += calibration::GRAIN_ENERGY_KJ;
                    sim.total_energy_intake_kj += calibration::GRAIN_ENERGY_KJ;
                    // 6.2: forage-success counter for the metrics dashboard.
                    sim.grains_consumed += 1;
                    // 4.2 spatial memory: committing food to memory is coupled
                    // to finding it (within MEMORY_FOUND_DIST_M = consumption
                    // radius). Upsert with strength 1.0, LRU-evict at cap.
                    remember_food(&mut memory.slots, *g_pos);
                    break;
                }
            }
        }
    }

    // Sync predator positions from physics.
    for (_id, (pos, _pred, phys_handle)) in sim
        .world
        .query_mut::<(&mut Position, &Predator, &PhysicsHandle)>()
    {
        if let Some(rb) = sim.physics.get_body(phys_handle.0) {
            let rb_pos = rb.translation();
            pos.0.x = rb_pos.x as f64;
            pos.0.y = rb_pos.y as f64;
        }
    }

    // Sprint 1 (Audit 5): `resolve_contact` must use the POST-physics agent
    // positions, not the stale map built before `physics.step()` above. The
    // original code passed the pre-step map, so contact was tested against
    // where agents were one solver step ago — a predator chasing at 10 m/s
    // moves 8 cm per step, which is inside the 1.0 m contact threshold's slack
    // but still a real position error that delayed captures and let near-misses
    // flicker. Rebuild the map from the just-synced `Position` components.
    let mut post_step_positions: FxHashMap<Entity, Vector2<f64>> = FxHashMap::default();
    for (id, (pos, _vel, _meta)) in sim
        .world
        .query::<(&Position, &Velocity, &Metabolism)>()
        .iter()
    {
        post_step_positions.insert(id, pos.0);
    }

    // 2.2 contact resolution (kills + cooldowns).
    predator::resolve_contact(sim, &post_step_positions);

    // 2.2b: expire predators whose randomized 5-15 s lifetime elapsed — remove
    // physics body + entity and log a `RemovePredator` ground-truth event.
    let expired_preds = predator::tick_lifetimes(sim, dt);
    if !expired_preds.is_empty() {
        let frame = sim.time.frame;
        for (id, uid) in expired_preds {
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

    // 2.4: execute natural deaths (despawn + remove physics body).
    for id in to_despawn {
        let handle = sim.world.get::<&PhysicsHandle>(id).ok().map(|h| h.0);
        // 7.2: energy removed from the live pool when the agent despawns.
        if let Ok(meta) = sim.world.get::<&Metabolism>(id) {
            sim.total_energy_lost_at_death_kj += meta.energy_kj;
        }
        sim.world.despawn(id).ok();
        if let Some(h) = handle {
            sim.physics.remove_body(h);
        }
        sim.deaths += 1;
    }

    // 2.4: immigration — keep the population above the minimum.
    // 4.2: gated by config for deterministic single-bird tests.
    // 4.3: spawn into obstacle-free points so new arrivals aren't pinned
    // inside a building collider.
    // Sprint 2 (Audit 5): capped at `max_agents` — immigration must never push
    // the population over the configured hard limit (no live pool overrun).
    let live = sim.world.query::<&Metabolism>().iter().count();
    if sim.config.immigration_enabled && live < calibration::MIN_POPULATION {
        let missing = calibration::MIN_POPULATION
            .min(sim.config.max_agents)
            .saturating_sub(live);
        for _ in 0..missing {
            let Some(pos) = avian_core::Simulation::random_free_point(
                sim.config.world_width,
                sim.config.world_height,
                &sim.obstacles,
                &mut sim.rng,
            ) else {
                // 4.3 exhaust: no obstacle-free point found; skip rather than
                // spawn inside a collider.
                break;
            };
            let uid = sim.next_uid_str();
            let e = spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
            // 7.2: energy carried in by the respawned agent.
            if let Ok(meta) = sim.world.get::<&Metabolism>(e) {
                sim.total_energy_inflow_spawn_kj += meta.energy_kj;
            }
        }
    }

    // Consume grains.
    let mut to_despawn_grains: Vec<Entity> = Vec::new();
    for g_id in grains_to_consume {
        if let Ok(mut g) = sim.world.get::<&mut Grain>(g_id) {
            g.amount = g.amount.saturating_sub(1);
            if g.amount == 0 {
                to_despawn_grains.push(g_id);
            }
        }
    }
    for g_id in to_despawn_grains {
        sim.world.despawn(g_id).ok();
        // Audit 3 (Phase 2): the grain set changed → visible-grain caches must
        // recompute next tick.
        sim.grains_version = sim.grains_version.wrapping_add(1);
    }
}
