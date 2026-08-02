/// 2.7 acceptance: the `sick` flag fires for low-vitality agents (vitality <
/// 0.3), sick agents move 50% slower (including fleeing), and sick agents are
/// captured first when a predator is present.

use avian_core::{Simulation, SimulationConfig};
use avian_core::components::{Age, AgentUid, Heading, Metabolism, Position};
use avian_agent::systems::run_systems;
use avian_agent::gerontology::spawn_agent;
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

/// Pre-populate the world to MIN_POPULATION (10) with dummy agents placed
/// OUTSIDE the center agent's spatial-query block. `query_k_nearest` fetches a
/// ±5-cell (10 m) block and does NOT radius-filter, so dummies must be >10 m
/// away in X to never appear as boids neighbors of the center agent.
fn populate_dummies(sim: &mut Simulation) {
    let dummy_positions = [
        [2.0, 2.0], [2.0, 19.0], [4.0, 2.0], [4.0, 19.0],
        [28.0, 2.0], [28.0, 19.0], [30.0, 2.0], [30.0, 19.0],
    ];
    for [x, y] in dummy_positions {
        let uid = sim.next_uid_str();
        spawn_agent(&mut sim.world, &mut sim.rng, Vector2::new(x, y), &mut sim.physics, uid);
    }
}

fn force_sick(sim: &mut Simulation, e: hecs::Entity) {
    // vitality_at(8.0) = exp(-ln2 * (8/4)^2) = 0.0625 < 0.3 → sick.
    sim.world.get::<&mut Age>(e).unwrap().years = 8.0;
}

fn force_healthy(sim: &mut Simulation, e: hecs::Entity) {
    // vitality_at(1.0) ≈ 0.84 > 0.3 → healthy.
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
}

/// Point the agent's heading at `target` so the target is never inside the
/// FOV blind spot — the tests must not depend on the RNG's heading draw.
fn face(sim: &mut Simulation, e: hecs::Entity, target: Vector2<f64>) {
    let p = sim.world.get::<&Position>(e).unwrap().0;
    let dir = target - p;
    sim.world.get::<&mut Heading>(e).unwrap().0 = dir.y.atan2(dir.x);
}

#[test]
fn test_sick_slows_movement_and_flags() {
    // Speed of a lone center agent in an identical scene, once sick and once
    // healthy. The scene is boids-free for the center agent (8 dummies beyond
    // its query block), so the contrast isolates the 0.5 sick multiplier.
    fn center_speed(is_sick: bool) -> (f64, String) {
        let mut sim = Simulation::new(11, SimulationConfig::default());
        populate_dummies(&mut sim);
        sim.spawn_grain_entity(Vector2::new(19.0, 10.5), 10);
        let uid = sim.next_uid_str();
        let e = spawn_agent(&mut sim.world, &mut sim.rng, Vector2::new(16.0, 10.5), &mut sim.physics, uid.clone());
        if is_sick {
            force_sick(&mut sim, e);
        } else {
            force_healthy(&mut sim, e);
        }
        face(&mut sim, e, Vector2::new(19.0, 10.5));
        sim.world.get::<&mut Metabolism>(e).unwrap().energy_kj = 3.0;

        let mut exporter = TelemetryExporter::new(usize::MAX);
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let snap = sim.snapshot();
        let agent = snap.agents.iter().find(|a| a.uid == uid).unwrap();
        let speed = (agent.vel[0].powi(2) + agent.vel[1].powi(2)).sqrt();
        (speed, agent.fsm_state.clone())
    }

    let (healthy_speed, healthy_fsm) = center_speed(false);
    let (sick_speed, sick_fsm) = center_speed(true);

    assert_eq!(sick_fsm, "Sick", "sick agent should be in Sick state");
    assert_ne!(healthy_fsm, "Sick", "healthy agent should not be in Sick state");

    let ratio = healthy_speed / sick_speed;
    assert!(
        (1.6..=2.4).contains(&ratio),
        "expected ~2x speed gap (sick {sick_speed:.2}, healthy {healthy_speed:.2}), ratio {ratio:.2}"
    );
    assert!(
        sick_speed < 1.0,
        "sick speed should be below the 1.2 m/s walk speed (got {sick_speed:.2})"
    );
    assert!(
        healthy_speed > 1.0,
        "healthy speed should reach the ~1.2 m/s walk speed (got {healthy_speed:.2})"
    );
}

#[test]
fn test_sick_captured_before_healthy() {
    let config = SimulationConfig {
        predator_expiry: false,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(23, config);
    // Dummies fill the population but stay out of the predator's reach at the
    // start (sick at 13 m and healthy at 23 m keep them >10 m from the dummies'
    // block at the far left/right corners).
    populate_dummies(&mut sim);

    // Predator between them; sick agent closer (3 m), healthy farther (7 m).
    // Both inside the 8 m detection radius → both flee; the predator chases
    // the nearest (sick), who flees at half speed → caught first.
    let uid_sick = sim.next_uid_str();
    let e_sick = spawn_agent(&mut sim.world, &mut sim.rng, Vector2::new(13.0, 10.5), &mut sim.physics, uid_sick.clone());
    let uid_healthy = sim.next_uid_str();
    let e_healthy = spawn_agent(&mut sim.world, &mut sim.rng, Vector2::new(23.0, 10.5), &mut sim.physics, uid_healthy.clone());
    force_sick(&mut sim, e_sick);
    force_healthy(&mut sim, e_healthy);
    // Point each away from the predator so the hawk is never in the blind spot.
    face(&mut sim, e_sick, Vector2::new(2.0, 10.5));
    face(&mut sim, e_healthy, Vector2::new(30.0, 10.5));
    sim.spawn_predator(Vector2::new(16.0, 10.5));

    let mut exporter = TelemetryExporter::new(usize::MAX);
    let alive = vec![uid_sick.clone(), uid_healthy.clone()];
    let mut first_captured: Option<String> = None;

    for _ in 0..6000 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let now_alive: Vec<String> = sim
            .world
            .query::<&AgentUid>()
            .iter()
            .filter(|(id, _)| sim.world.get::<&Metabolism>(*id).is_ok())
            .map(|(_, uid)| uid.0.clone())
            .collect();
        for uid in &alive {
            if !now_alive.contains(uid) {
                first_captured = Some(uid.clone());
                break;
            }
        }
        if first_captured.is_some() {
            break;
        }
    }

    assert_eq!(
        first_captured.as_deref(),
        Some(uid_sick.as_str()),
        "sick agent should be captured before the healthy one (got {first_captured:?})"
    );
}
