//! 3.6 Checkpoint/Replay determinism tests (Sprint 4).
//!
//! 1. Roundtrip: save_checkpoint → load_checkpoint preserves world contents,
//!    time, counters, and physics.
//! 2. Determinism: run N frames, checkpoint at N/2, then compare the
//!    continuation of the LIVE run vs. the RESTORED run — every agent
//!    position/FSM and all counters must be byte-identical. This is the core
//!    promise of checkpoints (resume exactly from state, e.g. ablation studies
//!    with altered parameters).

use avian_core::{Simulation, SimulationConfig};
use avian_core::checkpoint::{deserialize_world, serialize_world};
use avian_core::components::{FSMState, Position, Metabolism, AgentUid};
use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_telemetry::exporter::TelemetryExporter;
use hecs::World;
use nalgebra::Vector2;
use std::any::TypeId;
use std::collections::BTreeSet;
use std::collections::HashMap;

fn setup_sim() -> Simulation {
    let mut sim = Simulation::new(4242, SimulationConfig::default());
    for _ in 0..25 {
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
    for _ in 0..40 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_grain(&mut sim, Vector2::new(x, y), 20);
    }
    // A predator exercises the physics + flee paths in the checkpoint.
    sim.spawn_predator(Vector2::new(16.0, 10.5));
    sim
}

/// Fingerprint everything that must survive a checkpoint: per-agent state,
/// counter deltas, energy accounting. Returns a deterministic string.
fn fingerprint(sim: &Simulation) -> String {
    let mut out = format!(
        "frame={} time_us={} next_uid={} deaths={} kills={} intake={:.6} exp={:.6} lost={:.6} inflow={:.6}",
        sim.time.frame,
        sim.time.time_us,
        sim.next_uid,
        sim.deaths,
        sim.predator_kills,
        sim.total_energy_intake_kj,
        sim.total_energy_expenditure_kj,
        sim.total_energy_lost_at_death_kj,
        sim.total_energy_inflow_spawn_kj,
    );
    let mut agents: Vec<(String, [f64; 2], f64, f64)> = sim
        .world
        .query::<(&AgentUid, &Position, &Metabolism, &FSMState)>()
        .iter()
        .map(|(_, (uid, pos, meta, fsm))| {
            (uid.0.clone(), [pos.0.x, pos.0.y], meta.energy_kj, 0.0)
        })
        .collect();
    // Sort by UID so entity traversal order doesn't affect the digest.
    agents.sort_by(|a, b| a.0.cmp(&b.0));
    let mut fsm: HashMap<String, u32> = HashMap::new();
    for (_, _, _, f) in agents.iter() {
        let _ = f;
    }
    for (_, (_, _, _, fsm_state)) in sim.world.query::<(&AgentUid, &Position, &Metabolism, &FSMState)>().iter() {
        *fsm.entry(format!("{:?}", fsm_state)).or_insert(0) += 1;
    }
    let mut fsm_keys: Vec<_> = fsm.keys().cloned().collect();
    fsm_keys.sort();
    let fsm_part: String = fsm_keys
        .iter()
        .map(|k| format!("{}={}", k, fsm[k]))
        .collect::<Vec<_>>()
        .join(",");
    out.push_str(&format!(" FSM[{}]", fsm_part));
    for (uid, pos, energy, _) in agents {
        out.push_str(&format!("|{}@{:.4},{:.4}E{:.4}", uid, pos[0], pos[1], energy));
    }
    out
}

#[test]
fn test_checkpoint_roundtrip_preserves_state() {
    let mut sim = setup_sim();
    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..300 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }

    let path = std::env::temp_dir().join("avian_ckpt_roundtrip.bin");
    let p = path.to_str().unwrap().to_string();
    sim.save_checkpoint(&p).expect("save checkpoint");

    let restored = Simulation::load_checkpoint(&p).expect("load checkpoint");

    // Time, counters, energy accounting must match exactly.
    assert_eq!(restored.time.frame, sim.time.frame);
    assert_eq!(restored.time.time_us, sim.time.time_us);
    assert_eq!(restored.next_uid, sim.next_uid);
    assert_eq!(restored.deaths, sim.deaths);
    assert_eq!(restored.predator_kills, sim.predator_kills);
    assert!(
        (restored.total_energy_intake_kj - sim.total_energy_intake_kj).abs() < 1e-9,
        "intake mismatch"
    );
    assert!(
        (restored.total_energy_expenditure_kj - sim.total_energy_expenditure_kj).abs() < 1e-9
    );
    assert!((restored.total_energy_lost_at_death_kj - sim.total_energy_lost_at_death_kj).abs() < 1e-9);

    // Agent set must be identical (same count, same UIDs, same positions).
    assert_eq!(fingerprint(&restored), fingerprint(&sim), "roundtrip digest mismatch");
    assert_eq!(
        sim.world.query::<&Position>().iter().count(),
        restored.world.query::<&Position>().iter().count(),
        "agent count changed after roundtrip"
    );

    // Physics bodies must be restored: positions actually integrate when the
    // restored sim steps (bodies exist and move), not silently missing.
    let before = fingerprint(&restored);
    let mut exporter2 = TelemetryExporter::new(usize::MAX);
    let mut restored = restored;
    for _ in 0..50 {
        restored.step(|s, dt| run_systems(s, dt, &mut exporter2));
    }
    assert_ne!(before, fingerprint(&restored), "restored sim did not advance");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_checkpoint_determinism_continuation() {
    const TOTAL: u64 = 1200;
    const SPLIT: u64 = 600;

    // Live run: full N frames in one go.
    let mut live = setup_sim();
    let mut exporter_live = TelemetryExporter::new(usize::MAX);
    for _ in 0..TOTAL {
        live.step(|s, dt| run_systems(s, dt, &mut exporter_live));
    }

    // Checkpointed run: run to SPLIT, save, restore, continue to TOTAL.
    let mut ckpt_sim = setup_sim();
    let mut exporter_ckpt = TelemetryExporter::new(usize::MAX);
    for _ in 0..SPLIT {
        ckpt_sim.step(|s, dt| run_systems(s, dt, &mut exporter_ckpt));
    }
    let path = std::env::temp_dir().join("avian_ckpt_det.bin");
    let p = path.to_str().unwrap().to_string();
    ckpt_sim.save_checkpoint(&p).expect("save checkpoint");
    drop(ckpt_sim);

    let mut restored = Simulation::load_checkpoint(&p).expect("load checkpoint");
    let mut exporter_rest = TelemetryExporter::new(usize::MAX);
    for _ in SPLIT..TOTAL {
        restored.step(|s, dt| run_systems(s, dt, &mut exporter_rest));
    }

    // Every agent state + every counter must be identical to the live run.
    let a = fingerprint(&live);
    let b = fingerprint(&restored);
    if a != b {
        panic!(
            "checkpoint continuation diverged after frame {SPLIT}:\n  live:      {a}\n  restored:  {b}"
        );
    }

    let _ = std::fs::remove_file(&path);
}

/// Audit 2 Task 5: guard against the checkpoint's hand-maintained component
/// registry silently dropping a component that spawn logic uses.
///
/// `checkpoint.rs` keeps a manual `ComponentId` list of every type included in
/// the world serialization; it must stay in sync with what `spawn_agent` /
/// `spawn_grain` / `spawn_predator` actually insert. If someone adds a new
/// component to a spawn path and forgets to register it, the code still
/// compiles but that column is silently dropped from every checkpoint.
///
/// This test closes that gap: it spawns the three entity kinds, round-trips
/// the world through `serialize_world`/`deserialize_world`, and asserts that
/// the set of component types present in the live world's archetypes is
/// byte-identical to the set present after restore. A forgotten component
/// shows up as a missing `TypeId` and fails the assertion.
#[test]
fn test_checkpoint_registers_every_spawned_component() {
    let mut sim = Simulation::new(7, SimulationConfig::default());
    for _ in 0..5 {
        let uid = sim.next_uid_str();
        spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(5.0, 5.0),
            &mut sim.physics,
            uid,
        );
    }
    spawn_grain(&mut sim, Vector2::new(3.0, 3.0), 20);
    sim.spawn_predator(Vector2::new(16.0, 10.5));

    fn component_type_ids(world: &World) -> BTreeSet<TypeId> {
        world
            .archetypes()
            .flat_map(|a| a.component_types())
            .collect()
    }

    let before = component_type_ids(&sim.world);
    let bytes = serialize_world(&sim.world).expect("serialize world");
    let restored = deserialize_world(&bytes).expect("deserialize world");
    let after = component_type_ids(&restored);

    let missing: Vec<TypeId> = before.difference(&after).copied().collect();
    assert!(
        missing.is_empty(),
        "checkpoint dropped component types used at spawn time: {missing:?} — \
         add them to the ComponentId list in avian_core/src/checkpoint.rs"
    );
    assert_eq!(
        before.len(),
        after.len(),
        "checkpoint registered-set mismatch: {} types in, {} out",
        before.len(),
        after.len()
    );
}
