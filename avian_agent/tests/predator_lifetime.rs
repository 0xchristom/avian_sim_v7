//! 2.2b acceptance: a predator despawns after a randomized 5-15 s lifetime, and
//! its disappearance is logged as a `RemovePredator` ground-truth event.

use avian_core::{Simulation, SimulationConfig};
use avian_core::components::Predator;
use avian_core::events::Event;
use avian_agent::systems::run_systems;
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

#[test]
fn test_predator_expires_within_5_15s_and_logs_event() {
    // 6.2: fill_meals off — this test isolates the LIFETIME expiry path (the
    // 3-meal despawn has its own acceptance test).
    let mut sim = Simulation::new(
        42,
        SimulationConfig {
            predator_fill_meals: false,
            ..SimulationConfig::default()
        },
    );
    sim.spawn_predator(Vector2::new(16.0, 10.5));
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut expiry_frame: Option<u32> = None;
    for frame in 0..=15 * 120 + 10 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        if sim.world.query::<&Predator>().iter().next().is_none() {
            expiry_frame = Some(frame);
            break;
        }
    }

    let frame = expiry_frame.expect("predator never despawned");
    let elapsed_s = frame as f64 / 120.0;
    assert!(
        (5.0..=15.0).contains(&elapsed_s),
        "predator lived {elapsed_s:.1}s — outside the 5-15 s window"
    );

    // The disappearance was logged as a RemovePredator event (survives in
    // events_log until the next tick's drain).
    let has_remove = sim
        .events_log
        .iter()
        .any(|(_, e)| matches!(e, Event::RemovePredator(_)));
    assert!(has_remove, "predator despawn was not logged as RemovePredator");
}

#[test]
fn test_predator_lifetime_visible_in_snapshot() {
    let mut sim = Simulation::new(
        7,
        SimulationConfig {
            predator_fill_meals: false,
            ..SimulationConfig::default()
        },
    );
    sim.spawn_predator(Vector2::new(16.0, 10.5));
    let mut exporter = TelemetryExporter::new(usize::MAX);

    sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    let snap = sim.snapshot();
    assert_eq!(snap.predators.len(), 1);
    let p = &snap.predators[0];
    assert!(
        (5.0..=15.0).contains(&p.lifetime_remaining_s),
        "snapshot lifetime {:.2}s outside 5-15 s",
        p.lifetime_remaining_s
    );

    // After ~2 s of stepping, the countdown decreased.
    for _ in 0..240 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    let snap2 = sim.snapshot();
    if let Some(p2) = snap2.predators.first() {
        assert!(p2.lifetime_remaining_s < p.lifetime_remaining_s);
    }
}
