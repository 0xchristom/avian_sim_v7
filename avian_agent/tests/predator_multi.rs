//! 2.2 regression: multiple predators, including predators deployed AFTER the
//! sim has been running (physics body slots may have been freed and reused with
//! a new generation — the handle must preserve index + generation or the body
//! is frozen in place forever).

use avian_core::{Simulation, SimulationConfig};
use avian_agent::systems::run_systems;
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

#[test]
fn multiple_predators_all_move() {
    let config = SimulationConfig { predator_expiry: false, predator_fill_meals: false, ..SimulationConfig::default() };
    let mut sim = Simulation::new(7, config);

    // Two predators far apart, no agents — both should PATROL.
    let e1 = sim.spawn_predator(Vector2::new(5.0, 5.0));
    let e2 = sim.spawn_predator(Vector2::new(25.0, 16.0));
    let u1 = sim.world.get::<&avian_core::components::AgentUid>(e1).unwrap().0.clone();
    let u2 = sim.world.get::<&avian_core::components::AgentUid>(e2).unwrap().0.clone();

    let mut exporter = TelemetryExporter::new(usize::MAX);
    let mut moved1 = false;
    let mut moved2 = false;
    for _ in 0..600 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let snap = sim.snapshot();
        if let Some(p) = snap.predators.iter().find(|p| p.uid == u1) {
            if (p.pos[0] - 5.0).abs() + (p.pos[1] - 5.0).abs() > 0.5 { moved1 = true; }
        }
        if let Some(p) = snap.predators.iter().find(|p| p.uid == u2) {
            if (p.pos[0] - 25.0).abs() + (p.pos[1] - 16.0).abs() > 0.5 { moved2 = true; }
        }
    }
    assert!(moved1, "predator 1 never moved");
    assert!(moved2, "predator 2 never moved");
}

#[test]
fn sequentially_deployed_predators_move() {
    // Server default config: predator_expiry ON. Run a while so agent despawns
    // free physics-body slots, THEN deploy more predators — they must move too.
    let mut sim = Simulation::new(9, SimulationConfig::default());
    let mut exporter = TelemetryExporter::new(usize::MAX);
    sim.spawn_predator(Vector2::new(4.0, 4.0));

    // ~6 sim-seconds — immigration/despawns reuse body slots with new gens.
    for _ in 0..720 { sim.step(|s, dt| run_systems(s, dt, &mut exporter)); }

    let e2 = sim.spawn_predator(Vector2::new(8.0, 8.0));
    let e3 = sim.spawn_predator(Vector2::new(24.0, 16.0));
    let u2 = sim.world.get::<&avian_core::components::AgentUid>(e2).unwrap().0.clone();
    let u3 = sim.world.get::<&avian_core::components::AgentUid>(e3).unwrap().0.clone();
    let p2 = sim.world.get::<&avian_core::components::Position>(e2).unwrap().0;
    let p3 = sim.world.get::<&avian_core::components::Position>(e3).unwrap().0;

    let mut moved2 = false;
    let mut moved3 = false;
    for _ in 0..600 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let snap = sim.snapshot();
        if let Some(p) = snap.predators.iter().find(|p| p.uid == u2) {
            if (p.pos[0] - p2.x).abs() + (p.pos[1] - p2.y).abs() > 0.5 { moved2 = true; }
        }
        if let Some(p) = snap.predators.iter().find(|p| p.uid == u3) {
            if (p.pos[0] - p3.x).abs() + (p.pos[1] - p3.y).abs() > 0.5 { moved3 = true; }
        }
    }
    assert!(moved2, "predator deployed after a despawn never moved");
    assert!(moved3, "second post-despawn predator never moved");
}
