use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::events::{Event, SpawnGrainRequest};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;

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
    assert_eq!(
        snap1, snap2,
        "Snapshots diverge at 1000 frames! Determinism broken."
    );
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
                let ev = Event::SpawnGrain(SpawnGrainRequest {
                    pos: [5.0, 5.0],
                    count: 10,
                });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            150 => {
                let ev = Event::SpawnGrain(SpawnGrainRequest {
                    pos: [20.0, 15.0],
                    count: 5,
                });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            300 => {
                let ev = Event::SpawnGrain(SpawnGrainRequest {
                    pos: [10.0, 3.0],
                    count: 8,
                });
                sim1.inject_event(ev.clone());
                sim2.inject_event(ev);
            }
            _ => {}
        }
    }

    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    assert_eq!(
        snap1, snap2,
        "Event-injected runs diverged! Determinism broken."
    );
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
        assert!(
            !a.uid.contains('%'),
            "raw arena index leaked into uid: {}",
            a.uid
        );
    }
}

// Sprint 1 (Audit 5 4.5): the predator contact roll resolves post-physics
// positions in a stable entity-id order; reproduction (nested heap pushes that
// reorder by weight) must not leak RNG streams. Both runs bit-match with a
// predator present and reproduction enabled.
#[test]
fn test_bit_perfect_with_predator_contact_and_reproduction() {
    let cfg = SimulationConfig {
        predator_expiry: false,
        predator_fill_meals: false,
        ..SimulationConfig::default()
    };
    let build = || {
        let mut sim = Simulation::new(7, cfg.clone());
        let center = nalgebra::Vector2::new(16.0, 10.5);
        for _ in 0..20 {
            let x = center.x + sim.rng.gen_range(-5.0..5.0);
            let y = center.y + sim.rng.gen_range(-5.0..5.0);
            let uid = sim.next_uid_str();
            spawn_agent(
                &mut sim.world,
                &mut sim.rng,
                nalgebra::Vector2::new(x, y),
                &mut sim.physics,
                uid,
            );
        }
        sim.spawn_predator(center);
        sim
    };

    let mut sim1 = build();
    let mut sim2 = build();
    let mut exp1 = TelemetryExporter::new(5000);
    let mut exp2 = TelemetryExporter::new(5000);

    for _ in 0..5000 {
        sim1.step(|s, dt| run_systems(s, dt, &mut exp1));
        sim2.step(|s, dt| run_systems(s, dt, &mut exp2));
    }

    assert!(
        sim1.predator_kills > 0,
        "predator should capture at least once"
    );
    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    assert_eq!(
        snap1, snap2,
        "Predator-contact runs diverged! Determinism broken."
    );
}

// Sprint 1 (Audit 5) acceptance: "The determinism test passes for at least
// 10,000 steps." The 5,000-frame predator test above proves stability; this
// test pushes the same predator+reproduction scenario to 10,000 frames so the
// acceptance criterion is met by an actual test, not extrapolation.
#[test]
fn test_bit_perfect_10000_frames_predator_reproduction() {
    let cfg = SimulationConfig {
        predator_expiry: false,
        predator_fill_meals: false,
        ..SimulationConfig::default()
    };
    let build = || {
        let mut sim = Simulation::new(7, cfg.clone());
        let center = nalgebra::Vector2::new(16.0, 10.5);
        for _ in 0..20 {
            let x = center.x + sim.rng.gen_range(-5.0..5.0);
            let y = center.y + sim.rng.gen_range(-5.0..5.0);
            let uid = sim.next_uid_str();
            spawn_agent(
                &mut sim.world,
                &mut sim.rng,
                nalgebra::Vector2::new(x, y),
                &mut sim.physics,
                uid,
            );
        }
        sim.spawn_predator(center);
        sim
    };

    let mut sim1 = build();
    let mut sim2 = build();
    let mut exp1 = TelemetryExporter::new(10_000);
    let mut exp2 = TelemetryExporter::new(10_000);

    for _ in 0..10_000 {
        sim1.step(|s, dt| run_systems(s, dt, &mut exp1));
        sim2.step(|s, dt| run_systems(s, dt, &mut exp2));
    }

    assert!(
        sim1.predator_kills > 0,
        "predator should capture at least once"
    );
    assert_eq!(sim1.time.frame, 10_000);
    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    assert_eq!(
        snap1, snap2,
        "10,000-frame predator runs diverged! Determinism broken."
    );
}

// Sprint 5 (release gate): headless determinism must extend to the telemetry
// OUTPUT, not just the in-memory snapshot. Two identical runs (same seed/config,
// same injected events) must produce identical telemetry frames. Compare the
// CSV files as sorted line sets because `finish()` drains a HashMap whose
// iteration order is unspecified.
#[test]
fn test_telemetry_output_is_bit_identical() {
    use avian_telemetry::exporter::TelemetryExporter;

    fn run(output: &str) {
        let mut sim = Simulation::new(42, SimulationConfig::default());
        let mut exporter = TelemetryExporter::new(2000);
        exporter.open(std::path::Path::new(output)).unwrap();
        // Same event schedule into both runs so the deterministic input stream
        // is identical (2.5 replay log).
        for frame in 0..300u64 {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
            let ev = match frame {
                50 => Some(Event::SpawnGrain(SpawnGrainRequest {
                    pos: [5.0, 5.0],
                    count: 10,
                })),
                150 => Some(Event::SpawnGrain(SpawnGrainRequest {
                    pos: [20.0, 15.0],
                    count: 5,
                })),
                _ => None,
            };
            if let Some(ev) = ev {
                sim.inject_event(ev);
            }
        }
        exporter.finish();
    }

    let p1 = std::env::temp_dir().join("avian_det_tel_1.csv");
    let p2 = std::env::temp_dir().join("avian_det_tel_2.csv");
    let p1s = p1.to_str().unwrap().to_string();
    let p2s = p2.to_str().unwrap().to_string();
    run(&p1s);
    run(&p2s);

    let read_lines = |p: &str| -> Vec<String> {
        let content = std::fs::read_to_string(p).expect("telemetry file exists");
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        lines.sort();
        lines
    };
    let a = read_lines(&p1s);
    let b = read_lines(&p2s);

    let _ = std::fs::remove_file(&p1);
    let _ = std::fs::remove_file(&p2);

    assert!(
        a.len() > 1,
        "telemetry should contain frames plus the CSV header"
    );
    assert_eq!(
        a,
        b,
        "identical runs produced different telemetry output (frame count {} vs {})",
        a.len(),
        b.len()
    );
}
