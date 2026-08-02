use avian_core::{Simulation, SimulationConfig};
use avian_core::events::{Event, SpawnGrainRequest};
use avian_agent::systems::run_systems;
use avian_agent::gerontology::spawn_agent;
use avian_telemetry::exporter::TelemetryExporter;
use bincode;

fn setup_sim() -> Simulation {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    for i in 0..20 {
        let pos = nalgebra::Vector2::new(i as f64 * 1.5, 10.0);
        let uid = sim.next_uid_str();
        spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
    }
    sim
}

#[test]
fn test_bit_perfect_reproducibility_with_systems() {
    let mut sim1 = setup_sim();
    let mut sim2 = setup_sim();
    let mut exp1 = TelemetryExporter::new(100);
    let mut exp2 = TelemetryExporter::new(100);

    for _ in 0..100 {
        sim1.step(|s, dt| run_systems(s, dt, &mut exp1));
        sim2.step(|s, dt| run_systems(s, dt, &mut exp2));
    }

    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();

    assert_eq!(sim1.time.frame, 100, "Frame counter should be exactly 100");
    assert_eq!(snap1, snap2, "Snapshots diverge! Determinism broken.");
}

// 7.1: 1000-frame bit-perfect reproducibility.
#[test]
fn test_bit_perfect_1000_frames() {
    let mut sim1 = setup_sim();
    let mut sim2 = setup_sim();
    let mut exp1 = TelemetryExporter::new(1000);
    let mut exp2 = TelemetryExporter::new(1000);

    for _ in 0..1000 {
        sim1.step(|s, dt| run_systems(s, dt, &mut exp1));
        sim2.step(|s, dt| run_systems(s, dt, &mut exp2));
    }

    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    assert_eq!(sim1.time.frame, 1000);
    assert_eq!(snap1, snap2, "Snapshots diverge at 1000 frames! Determinism broken.");
}

// 7.1: with injected events the run must stay reproducible — events are part
// of the deterministic input stream (2.5 replay log).
#[test]
fn test_bit_perfect_with_injected_events() {
    let mut sim1 = setup_sim();
    let mut sim2 = setup_sim();
    let mut exp1 = TelemetryExporter::new(1000);
    let mut exp2 = TelemetryExporter::new(1000);

    for frame in 0..500 {
        sim1.step(|s, dt| run_systems(s, dt, &mut exp1));
        sim2.step(|s, dt| run_systems(s, dt, &mut exp2));

        // Same event schedule injected into both runs at the same frames.
        match frame {
            50 => {
                let ev = Event::SpawnGrain(SpawnGrainRequest { pos: [5.0, 5.0], count: 10 });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            150 => {
                let ev = Event::SpawnGrain(SpawnGrainRequest { pos: [20.0, 15.0], count: 5 });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            300 => {
                let ev = Event::SpawnGrain(SpawnGrainRequest { pos: [10.0, 3.0], count: 8 });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            _ => {}
        }
    }

    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    assert_eq!(snap1, snap2, "Event-injected runs diverged! Determinism broken.");
}

// 3.3: stable uids are deterministic per seed and follow the A{session}-{id}
// format (no raw arena indices leaking across runs).
#[test]
fn test_stable_uids_deterministic() {
    let sim1 = setup_sim();
    let sim2 = setup_sim();
    let snap1 = sim1.snapshot();
    let snap2 = sim2.snapshot();

    assert_eq!(snap1.agents.len(), snap2.agents.len());
    for (a, b) in snap1.agents.iter().zip(snap2.agents.iter()) {
        assert_eq!(a.uid, b.uid, "uid must be deterministic per seed");
        assert!(a.uid.starts_with("A0001-"), "uid format broken: {}", a.uid);
        assert!(!a.uid.contains('%'), "raw arena index leaked into uid: {}", a.uid);
    }
}
