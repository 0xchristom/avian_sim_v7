use avian_core::Simulation;
use avian_core::AgentSnapshot;
use avian_core::components::*;
use avian_core::calibration;
use crate::behavior_tree::{build_default_tree, AgentContext};
use crate::locomotion::HeadBobSystem;
use crate::perception::cone_cast;
use crate::metabolism::metabolism_system;
use crate::gerontology::{mortality_hazard, spawn_agent};
use crate::flocking;
use crate::predator;
use avian_telemetry::rlhf::{state_to_observation, RLReward};
use avian_telemetry::exporter::TelemetryExporter;
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
            .min_by(|a, b| a.1.strength.partial_cmp(&b.1.strength).unwrap())
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

/// Per-agent snapshot captured in the behavior loop and exported to telemetry
/// after physics, so rewards can include the frame's events (grain eaten,
/// capture, flee-success).
struct RlExportData {
    snap: AgentSnapshot,
    neighbor_pos: Vec<[f64; 2]>,
    grain_pos: Vec<[f64; 2]>,
    flock_count: usize,
}

pub fn run_systems(sim: &mut Simulation, dt: f64, exporter: &mut TelemetryExporter) {
    // 2.3 day/night cycle. A full day = DAY_LENGTH_SIM_S sim-seconds.
    sim.environment.time_of_day_hours += dt / calibration::DAY_LENGTH_SIM_S * 24.0;
    sim.environment.time_of_day_hours %= 24.0;
    let h = sim.environment.time_of_day_hours;
    // Smooth sinusoid: noon (12h) = 1.0, midnight (0h/24h) = 0.1.
    sim.environment.light_level = 0.1
        + 0.9 * (0.5 + 0.5 * (2.0 * std::f64::consts::PI * (h - 12.0) / 24.0).cos());

    // 4.4: stochastic weather scheduler (config-gated; no-op when disabled).
    crate::weather::update(sim);

    // 2.5: flush injected events to telemetry with their frame number
    // (ground-truth annotations).
    for (frame, ev) in sim.events_log.drain(..) {
        exporter.log_event(frame, &serde_json::to_string(&ev).unwrap_or_default());
    }

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
    sim.spatial_grid.clear();
    let mut positions = FxHashMap::default();
    let mut velocities = FxHashMap::default();
    for (id, pos) in sim.world.query::<&Position>().iter() {
        if sim.world.get::<&Velocity>(id).is_ok() && sim.world.get::<&Metabolism>(id).is_ok() {
            positions.insert(id, pos.0);
            if let Ok(vel) = sim.world.get::<&Velocity>(id) {
                velocities.insert(id, vel.0);
            }
            sim.spatial_grid.insert(id, pos.0);
        }
    }

    let mut grains: Vec<(Entity, Vector2<f64>, u32)> = Vec::new();
    for (id, (pos, grain)) in sim.world.query::<(&Position, &Grain)>().iter() {
        if grain.amount > 0 {
            grains.push((id, pos.0, grain.amount));
        }
    }

    // 2.2 detection pass: agent → flee direction for any visible predator.
    let threats = predator::collect_threats(sim);

    // 4.2 spatial memory: decay remembered slots and pick each agent's
    // memory-biased forage target (weighted by strength) BEFORE the main loop,
    // so the tree tick can consume it without bloating the 15-element query.
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
                        .unwrap()
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
    let mut agent_data_for_rl: Vec<RlExportData> = Vec::new();
    let mut to_despawn: Vec<Entity> = Vec::new();

    for (id, (pos, head, vel, meta, levy, fsm, mass, mobility, vision, head_bob, age, _phys_handle, uid, alarm, feather))
        in sim.world.query_mut::<(&mut Position, &mut Heading, &mut Velocity, &mut Metabolism, &mut LevyState,
            &mut FSMState, &Mass, &Mobility, &Vision, &mut HeadBob, &mut Age, &PhysicsHandle, &AgentUid, &mut Alarm, &mut FeatherCondition)>()
    {
        // 2.6 feather decay — rain multiplies the rate (hook for 4.4).
        let rain_mult = if sim.environment.weather == Weather::Rain {
            calibration::RAIN_FEATHER_DECAY_MULTIPLIER
        } else {
            1.0
        };
        feather.0 = (feather.0 - calibration::FEATHER_DECAY_RATE_S * dt * rain_mult).max(0.0);

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
        let bmr_kj_s =
            meta.bmr_watts * calibration::flight_mr_multiplier(v_mag) * heat_mult * wind_on_flight / 1000.0;
        let cot_kj_s = 12.5 * mass_kg * v_mag / 1000.0;
        let night_factor = if sim.environment.light_level < calibration::NIGHT_REST_LIGHT_THRESHOLD {
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
        let neighbors_raw = sim.spatial_grid.query_k_nearest(pos.0, 7, vision_range, &positions);
        let targets: Vec<(Entity, Vector2<f64>)> = neighbors_raw.iter().filter_map(|(e, _)| {
            positions.get(e).map(|p| (*e, *p))
        }).collect();
        // 4.3: line-of-sight occlusion — walls/buildings block neighbor AND
        // grain vision. `sim.physics` is a disjoint field from `sim.world`
        // (borrowed by query_mut above), so raycasting here is sound.
        let physics = &sim.physics;
        let occluded = |target: &Vector2<f64>, _dist: f64| -> bool {
            physics
                .cast_ray_to_static(pos.0, *target - pos.0, 1.0)
                .map_or(false, |toi| toi < 1.0 - calibration::LOS_BLOCK_EPS)
        };
        let visible_neighbors = cone_cast(pos.0, head.0, vision.fov_degrees, vision_range, &targets, &occluded);
        let visible_neighbor_entities: Vec<Entity> = visible_neighbors.iter().map(|(e, _, _)| *e).collect();
        let visible_neighbor_pos: Vec<[f64; 2]> = visible_neighbors.iter().map(|(_, p, _)| [p.x, p.y]).collect();

        let visible_grains: Vec<(Entity, Vector2<f64>, u32)> = grains.iter().filter(|(_, g_pos, _)| {
            let dir = *g_pos - pos.0;
            let dist = dir.norm();
            if dist > vision_range || dist < 1e-6 { return false; }
            let angle = dir.y.atan2(dir.x) - head.0;
            let norm_ang = ((angle + std::f64::consts::PI) % (2.0 * std::f64::consts::PI)) - std::f64::consts::PI;
            if norm_ang.abs() > vision.fov_degrees.to_radians() / 2.0 { return false; }
            // 4.3: hide grain behind a wall/building even inside the FOV cone.
            if physics
                .cast_ray_to_static(pos.0, *g_pos - pos.0, 1.0)
                .map_or(false, |toi| toi < 1.0 - calibration::LOS_BLOCK_EPS)
            {
                return false;
            }
            true
        }).cloned().collect();
        let visible_grain_pos: Vec<[f64; 2]> = visible_grains.iter().map(|(_, p, _)| [p.x, p.y]).collect();

        let fleeing = threats.contains_key(&id);
        let flee_dir = threats.get(&id).copied().unwrap_or(Vector2::zeros());
        alarm.0 = fleeing;
        let sick = age.vitality < calibration::SICK_VITALITY_THRESHOLD;
        // 3.2 flocking reward input: agents within the 2 m shaping radius.
        let flock_count = neighbors_raw.iter()
            .filter(|(_, d)| *d <= calibration::REWARD_FLOCK_NEIGHBOR_DIST_M)
            .count();

        let mut ctx = AgentContext {
            pos, head, vel, meta, fsm, levy, mass, mobility, vision, head_bob,
            neighbors: visible_neighbor_entities.clone(),
            grains: visible_grains.clone(),
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
        };

        let _ = tree.tick(&mut ctx);

        // 2.1 boids-as-force: sum steering onto the tree-selected velocity.
        // Suppressed while fleeing (a fleeing pigeon does not align with its
        // flock), while preening (a preening pigeon stands still), and when the
        // 5.2 scenario disables flocking (config.flocking_enabled).
        if sim.config.flocking_enabled
            && *ctx.fsm != FSMState::Fleeing
            && *ctx.fsm != FSMState::Preening
        {
            let boid_neighbors: Vec<(Vector2<f64>, Vector2<f64>, f64)> = neighbors_raw.iter()
                .filter_map(|(e, d)| match (positions.get(e), velocities.get(e)) {
                    (Some(p), Some(v)) if *d > 1e-6 => Some((*p, *v, *d)),
                    _ => None,
                })
                .collect();
            let weights = flocking::weights_for_state(*ctx.fsm, &flocking::default_weights());
            let steer = flocking::steering(ctx.pos.0, &boid_neighbors, &weights);
            ctx.vel.0 += steer;
        }

        // 2.7: sick agents move at SICK_SPEED_MULTIPLIER (incl. fleeing → more
        // vulnerable to predators).
        if ctx.sick {
            ctx.vel.0 *= calibration::SICK_SPEED_MULTIPLIER;
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

        let snap = AgentSnapshot {
            uid: uid.0.clone(),
            pos: [ctx.pos.0.x, ctx.pos.0.y], heading: ctx.head.0, vel: [ctx.vel.0.x, ctx.vel.0.y],
            mass_g: ctx.mass.current_g, age_years: age.years, energy_kj: ctx.meta.energy_kj, hunger: ctx.meta.hunger,
            fsm_state: format!("{:?}", ctx.fsm), head_offset: [ctx.head_bob.offset.x, ctx.head_bob.offset.y],
            alarm_triggered: alarm.0,
            sick,
            vitality: age.vitality,
        };
        agent_data_for_rl.push(RlExportData {
            snap,
            neighbor_pos: visible_neighbor_pos.clone(),
            grain_pos: visible_grain_pos.clone(),
            flock_count,
        });
    }

    // 3.2 flee-success tracking (separate pass — keeps the main loop query at
    // 15 elements, hecs' tuple limit): an alarm that clears without a capture
    // this frame is a safely-ended fleeing episode (+0.5 one-shot).
    let mut flee_success_uids: HashSet<String> = HashSet::new();
    for (_, (alarm_prev, alarm, uid)) in sim.world.query_mut::<(&mut AlarmPrev, &Alarm, &AgentUid)>() {
        let was_alarmed = alarm_prev.0;
        let fleeing = alarm.0;
        if was_alarmed && !fleeing {
            flee_success_uids.insert(uid.0.clone());
        }
        alarm_prev.0 = fleeing;
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
            if let Some(rb) = sim.physics.get_body_mut(handle.0) {
                let v = linvel + wind_drift;
                rb.set_linvel(nalgebra::Vector2::new(v.x as f32, v.y as f32), true);
            }
        }
    }
    let pred_moves = predator::plan_movement(sim, &positions);
    for (id, linvel) in pred_moves {
        if let Ok(handle) = sim.world.get::<&PhysicsHandle>(id) {
            if let Some(rb) = sim.physics.get_body_mut(handle.0) {
                let v = linvel + wind_drift;
                rb.set_linvel(nalgebra::Vector2::new(v.x as f32, v.y as f32), true);
            }
        }
    }

    sim.physics.step();

    // Sync agent positions from physics + grain consumption. Consuming agent
    // UIDs feed the 3.2 +1.0 grain reward at export.
    let mut grains_to_consume: Vec<Entity> = Vec::new();
    let mut consumed_set: HashSet<Entity> = HashSet::new();
    let mut consumed_uids: Vec<String> = Vec::new();
    
    for (_id, (pos, _head, phys_handle, meta, fsm, uid, memory))
        in sim.world.query_mut::<(&mut Position, &mut Heading, &PhysicsHandle, &mut Metabolism, &FSMState, &AgentUid, &mut MemorySlots)>()
    {
        if let Some(rb) = sim.physics.get_body(phys_handle.0) {
            let rb_pos = rb.translation();
            pos.0.x = rb_pos.x as f64;
            pos.0.y = rb_pos.y as f64;
        }

        if *fsm == FSMState::Foraging {
            for (g_id, g_pos, _) in &grains {
                let dist = (g_pos - pos.0).norm();
                if dist < 0.5 && !consumed_set.contains(g_id) {
                    grains_to_consume.push(*g_id);
                    consumed_set.insert(*g_id);
                    meta.crop_count = (meta.crop_count + 1).min(meta.crop_max);
                    meta.energy_kj += calibration::GRAIN_ENERGY_KJ;
                    sim.total_energy_intake_kj += calibration::GRAIN_ENERGY_KJ;
                    consumed_uids.push(uid.0.clone());
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
    for (_id, (pos, _pred, phys_handle)) in sim.world.query_mut::<(&mut Position, &Predator, &PhysicsHandle)>() {
        if let Some(rb) = sim.physics.get_body(phys_handle.0) {
            let rb_pos = rb.translation();
            pos.0.x = rb_pos.x as f64;
            pos.0.y = rb_pos.y as f64;
        }
    }

    // 2.2 contact resolution (kills + cooldowns). Captured UIDs feed the 3.2
    // -10.0 one-shot reward at export.
    let captured_uids = predator::resolve_contact(sim, &positions);

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
    let live = sim.world.query::<&Metabolism>().iter().count();
    if sim.config.immigration_enabled && live < calibration::MIN_POPULATION {
        let missing = calibration::MIN_POPULATION - live;
        for _ in 0..missing {
            let pos = avian_core::Simulation::random_free_point(
                sim.config.world_width,
                sim.config.world_height,
                &sim.obstacles,
                &mut sim.rng,
            );
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
    }

    // RLHF export: obs_v1 observation + 3.2 event-driven reward (sparse +
    // shaped, per-second terms already scaled by dt).
    let time_us = sim.time.time_us;
    let frame = sim.time.frame;
    let light_level = sim.environment.light_level;
    let predator_positions: Vec<[f64; 2]> = sim
        .world
        .query::<(&Position, &Predator)>()
        .iter()
        .map(|(_, (p, _))| [p.0.x, p.0.y])
        .collect();
    for data in agent_data_for_rl {
        let obs = state_to_observation(&data.snap, &data.neighbor_pos, &data.grain_pos, &predator_positions, light_level);
        let grain_eaten = consumed_uids.iter().any(|u| u == &data.snap.uid);
        let captured = captured_uids.iter().any(|u| u == &data.snap.uid);
        let flee_success = flee_success_uids.contains(&data.snap.uid);
        let reward = RLReward::compute(
            sim.config.dt as f32,
            data.snap.energy_kj as f32,
            calibration::MAX_ENERGY_KJ as f32,
            data.flock_count,
            data.snap.alarm_triggered,
            flee_success,
            grain_eaten,
            captured,
        );

        // 3.5 ground-truth event labels for this frame.
        let mut event_labels: Vec<String> = Vec::new();
        if grain_eaten { event_labels.push("grain_consumed".into()); }
        if captured { event_labels.push("captured".into()); }
        if flee_success { event_labels.push("flee_success".into()); }
        if data.snap.alarm_triggered { event_labels.push("predator_seen".into()); }

        exporter.push(avian_telemetry::exporter::TelemetryFrame {
            time_us,
            frame,
            uid: data.snap.uid,
            obs: obs.vector.to_vec(),
            reward: reward.total,
            alarm_triggered: data.snap.alarm_triggered,
            sick: data.snap.sick,
            reward_grain: reward.grain,
            reward_flocking: reward.flocking,
            reward_starvation: reward.starvation,
            reward_captured: reward.captured,
            reward_flee_success: reward.flee_success,
            fsm: data.snap.fsm_state.clone(),
            event_labels,
            next_fsm: String::new(),
        });
    }
}
