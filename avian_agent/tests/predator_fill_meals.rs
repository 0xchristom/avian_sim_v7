//! 6.2 acceptance: a predator with `predator_fill_meals` enabled despawns
//! after eating `predator_fill_meals_target` (3) pigeons — the "disappears
//! after eating 3 pigeons" request — and the disappearance is logged as a
//! `RemovePredator` ground-truth event (same path as lifetime expiry).

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::components::{Age, AgentUid, Position};
use avian_core::events::Event;
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

/// Pre-populate the world to MIN_POPULATION (10) with dummy agents placed far
/// from the encounter so they never interfere.
fn populate_dummies(sim: &mut Simulation) {
    let dummy_positions = [
        [2.0, 2.0],
        [2.0, 19.0],
        [4.0, 2.0],
        [4.0, 19.0],
        [28.0, 2.0],
        [28.0, 19.0],
        [30.0, 2.0],
        [30.0, 19.0],
    ];
    for [x, y] in dummy_positions {
        let uid = sim.next_uid_str();
        spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
    }
}

fn force_sick(sim: &mut Simulation, e: hecs::Entity) {
    // vitality_at(8.0) = exp(-ln2 * (8/4)^2) = 0.0625 < 0.3 → sick → half-speed
    // flight, so the predator can actually run the bird down.
    sim.world.get::<&mut Age>(e).unwrap().years = 8.0;
}

fn face_away(sim: &mut Simulation, e: hecs::Entity, from: Vector2<f64>) {
    let p = sim.world.get::<&Position>(e).unwrap().0;
    let dir = p - from;
    sim.world
        .get::<&mut avian_core::components::Heading>(e)
        .unwrap()
        .0 = dir.y.atan2(dir.x);
}

#[test]
fn test_predator_despawns_after_three_meals() {
    let config = SimulationConfig {
        predator_expiry: false,
        predator_fill_meals: true,
        predator_fill_meals_target: 3,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(17, config);
    populate_dummies(&mut sim);

    // Four sick pigeons clustered around the hawk — slow enough (half-speed
    // flight) that the predator can catch and eat three of them.
    let center = Vector2::new(16.0, 10.5);
    let mut meals = Vec::new();
    for i in 0..4 {
        let off = match i {
            0 => Vector2::new(-1.5, 0.0),
            1 => Vector2::new(1.5, 0.0),
            2 => Vector2::new(0.0, -1.5),
            _ => Vector2::new(0.0, 1.5),
        };
        let uid = sim.next_uid_str();
        let e = spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            center + off,
            &mut sim.physics,
            uid.clone(),
        );
        force_sick(&mut sim, e);
        face_away(&mut sim, e, center);
        meals.push(uid);
    }
    let predator_uid = {
        let e = sim.spawn_predator(center);
        sim.world.get::<&AgentUid>(e).unwrap().0.clone()
    };

    let mut exporter = TelemetryExporter::new(usize::MAX);
    let mut despawned = false;
    for _ in 0..15000 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        if sim
            .world
            .query::<&avian_core::components::Predator>()
            .iter()
            .next()
            .is_none()
        {
            despawned = true;
            break;
        }
    }

    assert!(despawned, "predator never despawned after the meal quota");
    assert!(
        sim.predator_kills >= 3,
        "predator killed {} pigeons — expected >=3 before despawn",
        sim.predator_kills
    );

    // The disappearance was logged as a RemovePredator event, matching the
    // lifetime-expiry plumbing (survives until the next tick's drain).
    let has_remove = sim
        .events_log
        .iter()
        .any(|(_, e, _)| matches!(e, Event::RemovePredator(r) if r.uid == predator_uid));
    assert!(
        has_remove,
        "3-meal despawn was not logged as RemovePredator"
    );
}
