use avian_core::Simulation;
use avian_core::components::*;
use avian_core::components::PhysicsHandle;
use crate::behavior_tree::{build_default_tree, AgentContext};
use crate::locomotion::HeadBobSystem;
use avian_telemetry::rlhf::{state_to_observation, RLReward};
use avian_telemetry::exporter::TelemetryExporter;
use hecs::Entity;
use nalgebra::Vector2;
use rustc_hash::FxHashMap;

pub fn spawn_grain(sim: &mut Simulation, pos: Vector2<f64>, amount: u32) -> Entity {
    sim.world.spawn((Position(pos), Grain { amount }))
}

pub fn run_systems(sim: &mut Simulation, dt: f64, exporter: &mut TelemetryExporter) {
    // 1. Clear and Update Spatial Grid
    sim.spatial_grid.clear();
    let mut positions = FxHashMap::default();
    for (id, pos) in sim.world.query::<&Position>().iter() {
        positions.insert(id, pos.0);
        sim.spatial_grid.insert(id, pos.0);
    }

    // Zbieramy ziarna
    let mut grains: Vec<(Entity, Vector2<f64>, u32)> = Vec::new();
    for (id, (pos, grain)) in sim.world.query::<(&Position, &Grain)>().iter() {
        if grain.amount > 0 {
            grains.push((id, pos.0, grain.amount));
        }
    }

    let tree = build_default_tree();

    // 2. BT, Percepcja i Fizyka
    let mut commands: Vec<(Entity, Vector2<f64>)> = Vec::new();

    for (id, (pos, head, vel, meta, levy, fsm, mass, vision, head_bob, _phys_handle)) in sim.world.query_mut::<(&mut Position, &mut Heading, &mut Velocity, &mut Metabolism, &mut LevyState, &mut FSMState, &Mass, &Vision, &mut HeadBob, &PhysicsHandle)>() {
        
        let mass_kg = mass.current_g / 1000.0;
        let v_mag = vel.0.norm();
        let bmr_kj_s = meta.bmr_watts / 1000.0;
        let cot_kj_s = 12.5 * mass_kg * v_mag / 1000.0;
        meta.energy_kj -= (bmr_kj_s + cot_kj_s) * dt;
        
        let blood_glucose = meta.gizzard_count as f64 * 0.5;
        meta.hunger = 0.6 * (1.0 - meta.crop_count as f64 / meta.crop_max as f64)
                    + 0.4 * (1.0 - blood_glucose / 5.0).max(0.0);

        let neighbors = sim.spatial_grid.query_k_nearest(pos.0, 7, &positions);
        
        let mut nearby_grains: Vec<(Entity, Vector2<f64>, u32)> = Vec::new();
        for (g_id, g_pos, g_amt) in &grains {
            let dist = (g_pos - pos.0).norm();
            if dist < 5.0 {
                nearby_grains.push((*g_id, *g_pos, *g_amt));
            }
        }
        nearby_grains.sort_by(|a, b| {
            (a.1 - pos.0).norm().partial_cmp(&(b.1 - pos.0).norm()).unwrap()
        });

        let mut ctx = AgentContext {
            pos, head, vel, meta, fsm, levy, mass, vision, head_bob,
            neighbors: neighbors.clone(),
            grains: nearby_grains.clone(),
            rng: &mut sim.rng,
        };

        let _ = tree.tick(&mut ctx);

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

    // Aplikuj komendy do Rapier2D
    for (id, linvel) in commands {
        if let Ok(handle) = sim.world.get::<&PhysicsHandle>(id) {
            if let Some(rb) = sim.physics.bodies.get_mut(handle.0) {
                rb.set_linvel(nalgebra::Vector2::new(linvel.x as f32, linvel.y as f32), true);
            }
        }
    }

    // Krok fizyki
    sim.physics.step();

    // Odczyt pozycji z Rapiera i ew. zjadanie ziaren
    let mut grains_to_consume: Vec<Entity> = Vec::new();
    
    for (_id, (pos, head, phys_handle, meta, fsm)) in sim.world.query_mut::<(&mut Position, &mut Heading, &PhysicsHandle, &mut Metabolism, &FSMState)>() {
        if let Some(rb) = sim.physics.bodies.get(phys_handle.0) {
            let rb_pos = rb.translation();
            pos.0.x = rb_pos.x as f64;
            pos.0.y = rb_pos.y as f64;
            head.0 = rb.rotation().angle() as f64;
        }

        if *fsm == FSMState::Foraging {
            for (g_id, g_pos, _) in &grains {
                let dist = (g_pos - pos.0).norm();
                if dist < 0.5 {
                    grains_to_consume.push(*g_id);
                    meta.crop_count += 1;
                    meta.energy_kj += 0.5;
                    break;
                }
            }
        }
    }

    // Usuń zjedzone ziarna
    for g_id in grains_to_consume {
        if let Ok(mut g) = sim.world.get::<&mut Grain>(g_id) {
            g.amount -= 1;
        }
    }

    // 3. Eksport Telemetrii RLHF
    let snap = sim.snapshot();
    for agent_snap in &snap.agents {
        let _obs = state_to_observation(agent_snap);
        let _reward = RLReward::compute(agent_snap.energy_kj as f32, 60.0, 1.0, false);
        
        exporter.push(avian_telemetry::exporter::TelemetryFrame {
            time_us: snap.time_us,
            frame: snap.frame,
        });
    }
}