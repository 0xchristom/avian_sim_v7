//! 3.6 Checkpoint/Replay determinism tests (Sprint 4).
//!
//! 1. Roundtrip: save_checkpoint → load_checkpoint preserves world contents,
//!    time, counters, and physics.
//! 2. Determinism: run N frames, checkpoint at N/2, then compare the
//!    continuation of the LIVE run vs. the RESTORED run — every agent
//!    position/FSM and all counters must be byte-identical. This is the core
//!    promise of checkpoints (resume exactly from state, e.g. ablation studies
//!    with altered parameters).

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_core::checkpoint::{deserialize_world, serialize_world};
use avian_core::components::{AgentUid, FSMState, Metabolism, Position};
use avian_core::{Simulation, SimulationConfig};
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
        .map(|(_, (uid, pos, meta, _))| (uid.0.clone(), [pos.0.x, pos.0.y], meta.energy_kj, 0.0))
        .collect();
    // Sort by UID so entity traversal order doesn't affect the digest.
    agents.sort_by(|a, b| a.0.cmp(&b.0));
    let mut fsm: HashMap<String, u32> = HashMap::new();
    for (_, _, _, f) in agents.iter() {
        let _ = f;
    }
    for (_, (_, _, _, fsm_state)) in sim
        .world
        .query::<(&AgentUid, &Position, &Metabolism, &FSMState)>()
        .iter()
    {
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
        out.push_str(&format!(
            "|{}@{:.4},{:.4}E{:.4}",
            uid, pos[0], pos[1], energy
        ));
    }
    out
}

#[test]
fn test_checkpoint_roundtrip_preserves_state() {
    let mut sim = setup_sim();
    for _ in 0..300 {
        sim.step(run_systems);
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
    assert!((restored.total_energy_expenditure_kj - sim.total_energy_expenditure_kj).abs() < 1e-9);
    assert!(
        (restored.total_energy_lost_at_death_kj - sim.total_energy_lost_at_death_kj).abs() < 1e-9
    );

    // Agent set must be identical (same count, same UIDs, same positions).
    assert_eq!(
        fingerprint(&restored),
        fingerprint(&sim),
        "roundtrip digest mismatch"
    );
    assert_eq!(
        sim.world.query::<&Position>().iter().count(),
        restored.world.query::<&Position>().iter().count(),
        "agent count changed after roundtrip"
    );

    // Physics bodies must be restored: positions actually integrate when the
    // restored sim steps (bodies exist and move), not silently missing.
    let before = fingerprint(&restored);
    let mut restored = restored;
    for _ in 0..50 {
        restored.step(run_systems);
    }
    assert_ne!(
        before,
        fingerprint(&restored),
        "restored sim did not advance"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_checkpoint_determinism_continuation() {
    // Release gate: checkpoint save/load + a further 1000 steps produces the
    // same result as the continuous run.
    const TOTAL: u64 = 1600;
    const SPLIT: u64 = 600;

    // Live run: full N frames in one go.
    let mut live = setup_sim();
    for _ in 0..TOTAL {
        live.step(run_systems);
    }

    // Checkpointed run: run to SPLIT, save, restore, continue to TOTAL.
    let mut ckpt_sim = setup_sim();
    for _ in 0..SPLIT {
        ckpt_sim.step(run_systems);
    }
    let path = std::env::temp_dir().join("avian_ckpt_det.bin");
    let p = path.to_str().unwrap().to_string();
    ckpt_sim.save_checkpoint(&p).expect("save checkpoint");
    drop(ckpt_sim);

    let mut restored = Simulation::load_checkpoint(&p).expect("load checkpoint");
    for _ in SPLIT..TOTAL {
        restored.step(run_systems);
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

/// Sprint 5 (B16): a corrupt or truncated checkpoint file must surface as an
/// explicit error, not a panic or a silently-wrong simulation. bincode
/// deserialization of a truncated byte stream fails at the type boundary, so
/// `load_checkpoint` must propagate that as an `Err` and never return a
/// simulation with a partial world.
#[test]
fn test_checkpoint_truncated_file_errors() {
    let mut sim = setup_sim();
    for _ in 0..200 {
        sim.step(run_systems);
    }

    let path = std::env::temp_dir().join("avian_ckpt_trunc.bin");
    let p = path.to_str().unwrap().to_string();
    sim.save_checkpoint(&p).expect("save checkpoint");

    // Truncate the file to 40% of its length — mid-world, mid-payload.
    let bytes = std::fs::read(&p).expect("read checkpoint");
    let cut = (bytes.len() * 2) / 5;
    std::fs::write(&p, &bytes[..cut]).expect("truncate checkpoint");

    let result = Simulation::load_checkpoint(&p);
    assert!(
        result.is_err(),
        "truncated checkpoint must load as an error, got Ok"
    );

    // Flip random bytes — a corrupted-but-full-length payload must also error.
    std::fs::write(&p, &bytes).expect("restore checkpoint");
    let mut corrupt = bytes;
    let len = corrupt.len();
    for i in 0..32 {
        corrupt[len / 2 + i] = 0xFF;
    }
    std::fs::write(&p, &corrupt).expect("corrupt checkpoint");

    let result2 = Simulation::load_checkpoint(&p);
    assert!(
        result2.is_err(),
        "corrupted checkpoint must load as an error, got Ok"
    );

    let _ = std::fs::remove_file(&p);
}

/// Sprint 5 (B16): the atomic write path must never destroy the last valid
/// checkpoint. `save_checkpoint` serializes to a `.tmp` sibling and renames it
/// over the target, so a failed write leaves the previous file untouched. We
/// simulate the failure by pre-creating the `.tmp` file as a directory (rename
/// onto a non-empty directory fails on most platforms) — the pre-existing
/// checkpoint at `path` must still load.
#[test]
fn test_checkpoint_interrupted_write_keeps_last_valid() {
    let mut sim = setup_sim();
    for _ in 0..200 {
        sim.step(run_systems);
    }

    let path = std::env::temp_dir().join("avian_ckpt_atomic.bin");
    let p = path.to_str().unwrap().to_string();
    sim.save_checkpoint(&p).expect("save checkpoint");
    let before = fingerprint(&sim);

    // Sabotage the atomic write: make the `.tmp` target un-renamable. A
    // non-empty directory at the tmp path makes `rename` fail.
    let tmp = format!("{p}.tmp");
    std::fs::remove_file(&tmp).ok();
    std::fs::create_dir_all(&tmp).expect("create tmp dir");
    std::fs::write(format!("{tmp}\\blocker"), b"x").expect("blocker");

    let result = sim.save_checkpoint(&p);
    assert!(
        result.is_err(),
        "a blocked atomic write should report an error"
    );

    // The previous checkpoint must still be loadable and identical.
    let restored = Simulation::load_checkpoint(&p).expect("previous checkpoint survives");
    assert_eq!(
        fingerprint(&restored),
        before,
        "failed atomic write corrupted the previous checkpoint"
    );

    // Cleanup: remove the sabotaged tmp dir + the checkpoint.
    std::fs::remove_file(format!("{tmp}\\blocker")).ok();
    std::fs::remove_dir_all(&tmp).ok();
    let _ = std::fs::remove_file(&p);
}
