//! Audit 5a item 2: scripted population growth — integration test.
//!
//! Mirrors the server loop exactly: spawn the start population, then once per
//! `sim.step` (which advances the real frame counter) call `ScriptedGrowth::tick`.
//! The step closure is a no-op because this test isolates the SCHEDULE from
//! biology (metabolism deaths would otherwise perturb the exact totals); the
//! unit test in `scripted_population.rs` proves the schedule spawns through the
//! real `spawn_agent` path.

use avian_agent::scripted_population::{ScriptedGrowth, SCRIPTED_GROWTH_SCHEDULE};
use avian_core::components::{AgentUid, Metabolism};
use avian_core::{Simulation, SimulationConfig};

#[test]
fn server_loop_reaches_exact_totals_on_sim_time() {
    // dt = 1 s → one frame is one sim-second; 15 sim-min = 900 frames. Fast
    // enough for a full run while keying on frame × dt exactly like the real
    // default dt = 1/120.
    let mut sim = Simulation::new(
        7,
        SimulationConfig {
            dt: 1.0,
            scripted_population: true,
            ..SimulationConfig::default()
        },
    );
    let mut growth = ScriptedGrowth::default();
    growth.spawn_start(&mut sim);

    let count = |sim: &Simulation| sim.world.query::<&Metabolism>().iter().count();
    assert_eq!(count(&sim), 4, "starts at 4");

    // Run the whole schedule (900 frames = 15 sim-min) in the server order:
    // step (advances frame) then check the schedule.
    for frame in 1..=900u32 {
        sim.step(|_, _| {});
        let _ = growth.tick(&mut sim);
        match frame {
            120 => assert_eq!(count(&sim), 6, "total 6 at 2 sim-min"),
            300 => assert_eq!(count(&sim), 10, "total 10 at 5 sim-min"),
            600 => assert_eq!(count(&sim), 15, "total 15 at 10 sim-min"),
            900 => assert_eq!(count(&sim), 20, "total 20 at 15 sim-min"),
            _ => {}
        }
    }
    assert_eq!(count(&sim), 20, "holds at 20 after the last checkpoint");
}

/// The schedule must stay bit-deterministic: two same-seed runs spawn identical
/// UID sets at every checkpoint (the growth path consumes `sim.rng`, the same
/// seeded RNG as the rest of the sim — no HashMap-order randomness).
#[test]
fn schedule_is_deterministic_per_seed() {
    fn run() -> Vec<Vec<String>> {
        let mut sim = Simulation::new(
            99,
            SimulationConfig {
                dt: 1.0,
                scripted_population: true,
                ..SimulationConfig::default()
            },
        );
        let mut growth = ScriptedGrowth::default();
        growth.spawn_start(&mut sim);
        let mut snapshots = Vec::new();
        for frame in 1..=900u32 {
            sim.step(|_, _| {});
            let _ = growth.tick(&mut sim);
            if (frame % 120) == 0 {
                let mut uids: Vec<String> = sim
                    .world
                    .query::<&AgentUid>()
                    .iter()
                    .map(|(_, u)| u.0.clone())
                    .collect();
                uids.sort();
                snapshots.push(uids);
            }
        }
        snapshots
    }

    let a = run();
    let b = run();
    assert_eq!(a.len(), b.len());
    for (i, (sa, sb)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(sa, sb, "UID set diverged at checkpoint sample {i}");
    }
    // Sanity: UID sets grew and match the schedule. Samples land at frames
    // 120/240/360/480/600/720/840; checkpoints fire at 120/300/600/900, so the
    // sizes step 6→10→15 between samples as expected.
    let sizes: Vec<usize> = a.iter().map(|s| s.len()).collect();
    assert_eq!(sizes, vec![6, 6, 10, 10, 15, 15, 15]);
    assert_eq!(SCRIPTED_GROWTH_SCHEDULE.len(), 4, "schedule unchanged");
}
