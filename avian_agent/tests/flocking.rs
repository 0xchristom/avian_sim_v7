//! 2.1 acceptance: 30 agents form transient flocks (≥4 agents within 3m)
//! within the first 1000 frames of a headless run.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

fn setup_flock_sim() -> Simulation {
    let mut sim = Simulation::new(7, SimulationConfig::default());
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        let uid = sim.next_uid_str();
        spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
    }
    sim
}

/// Largest number of agents within 3m of any one agent (i.e. flock of N+1).
fn max_local_flock(snap: &avian_core::SimulationSnapshot) -> usize {
    let mut max = 0usize;
    for a in &snap.agents {
        let count = snap
            .agents
            .iter()
            .filter(|b| {
                let dx = a.pos[0] - b.pos[0];
                let dy = a.pos[1] - b.pos[1];
                let d = (dx * dx + dy * dy).sqrt();
                d <= 3.0 && d > 1e-6
            })
            .count();
        max = max.max(count);
    }
    max + 1 // +1 for the agent itself
}

#[test]
fn test_flocks_form_within_1000_frames() {
    let mut sim = setup_flock_sim();
    let mut exporter = TelemetryExporter::new(usize::MAX);
    let mut formed = false;
    let mut frame = 0u32;

    for _ in 0..1000 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        frame = sim.time.frame;
        if max_local_flock(&sim.snapshot()) >= 4 {
            formed = true;
            break;
        }
    }

    assert!(
        formed,
        "no flock of >=4 agents within 3m formed by frame {}",
        frame
    );
}
