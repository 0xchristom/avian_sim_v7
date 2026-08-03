//! 7.3 Performance benchmarks (Sprint 4).
//!
//! Run in release mode:
//!   cargo run --release -p avian_agent --bin bench -- agents 30 7200
//!   cargo run --release -p avian_agent --bin bench -- agents 500 3600
//!   cargo run --release -p avian_agent --bin bench -- export 100000
//!   cargo run --release -p avian_agent --bin bench -- snapshot 500 3600
//!   cargo run --release -p avian_agent --bin bench -- checkpoint 500 1200
//!
//! Targets (plan 7.3): 30 agents @120fps <1ms/frame; 500 agents @60fps
//! <5ms/frame; 100k telemetry frames exported in <30s. This measurement gates
//! 5.3 (custom physics) and drives 5.6 (caching) and 6.6 (render opts).
//!
//! Sprint 5 (tech task 6): structured performance counters — `snapshot` splits
//! the server broadcast path into sim-step / snapshot-build / JSON-serialize
//! ms and reports payload bytes; `checkpoint` measures save+load time at the
//! target population.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_core::{Simulation, SimulationConfig};
use avian_physics::PhysicsWorld;
use avian_telemetry::exporter::{TelemetryExporter, TelemetryFrame};
use nalgebra::Vector2;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("agents") => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
            let frames: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(7200);
            bench_agents(n, frames);
        }
        Some("export") => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
            bench_export(n);
        }
        Some("phys") => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
            let frames: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3600);
            bench_physics(n, frames);
        }
        Some("snapshot") => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
            let frames: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3600);
            bench_snapshot(n, frames);
        }
        Some("checkpoint") => {
            let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(500);
            let frames: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1200);
            bench_checkpoint(n, frames);
        }
        _ => {
            eprintln!("usage: bench <agents N frames | export N | phys N frames | snapshot N frames | checkpoint N frames>");
            std::process::exit(2);
        }
    }
}

fn bench_agents(n: usize, frames: u64) {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    // Audit 3 (Phase 1): `BENCH_DISABLED=1` measures the pure simulation cost
    // with telemetry fully inert (the intended headless/paused-RLHF mode).
    let mut exporter = if std::env::var("BENCH_DISABLED").is_ok() {
        TelemetryExporter::disabled()
    } else {
        TelemetryExporter::new(usize::MAX)
    };
    for _ in 0..n {
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
    for _ in 0..(n / 2).max(1) {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_grain(&mut sim, Vector2::new(x, y), 10);
    }
    if std::env::var("BENCH_NOGRAINS").is_ok() {
        let grains: Vec<_> = sim
            .world
            .query::<&avian_core::components::Grain>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for g in grains {
            let _ = sim.world.despawn(g);
        }
    }
    let grain_count = sim
        .world
        .query::<&avian_core::components::Grain>()
        .iter()
        .count();

    // Warmup (physics/scheduler JIT-ish effects, allocator caches).
    for _ in 0..300 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }

    let t0 = Instant::now();
    for _ in 0..frames {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    let dt = t0.elapsed().as_secs_f64();
    let ms_per_frame = dt * 1000.0 / frames as f64;
    let fps = frames as f64 / dt;
    println!(
        "{n} agents / {frames} frames ({grain_count} grains): {ms_per_frame:.3} ms/frame, {fps:.1} fps"
    );
    println!(
        "target {}: {}",
        if n <= 30 {
            "<1 ms/frame @120fps"
        } else {
            "<5 ms/frame @60fps"
        },
        if (n <= 30 && ms_per_frame < 1.0) || (n > 30 && ms_per_frame < 5.0) {
            "PASS"
        } else {
            "FAIL"
        }
    );
}

/// Sprint 5 (tech task 6): split the server broadcast path into its component
/// costs — simulation step, snapshot build, and JSON serialization — and report
/// the wire payload size. This measures the per-broadcast cost that `agents`
/// alone hides (B12: snapshot + JSON per client tick).
fn bench_snapshot(n: usize, frames: u64) {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut exporter = TelemetryExporter::disabled();
    for _ in 0..n {
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
    for _ in 0..(n / 2).max(1) {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_grain(&mut sim, Vector2::new(x, y), 10);
    }

    // Warmup (allocator caches + spatial grid steady state).
    for _ in 0..300 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }

    let mut sim_ms = 0.0;
    let mut snap_ms = 0.0;
    let mut ser_ms = 0.0;
    let mut payload_bytes = 0usize;
    for _ in 0..frames {
        let t0 = Instant::now();
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        sim_ms += t0.elapsed().as_secs_f64();

        let t1 = Instant::now();
        let snap = sim.snapshot();
        snap_ms += t1.elapsed().as_secs_f64();

        let t2 = Instant::now();
        let text = serde_json::to_string(&snap).expect("serialize snapshot");
        ser_ms += t2.elapsed().as_secs_f64();
        payload_bytes = text.len();
    }
    let f = frames as f64;
    println!("{n} agents / {frames} frames (snapshot broadcast path):");
    println!("  sim       {:.3} ms/frame", sim_ms * 1000.0 / f);
    println!("  snapshot  {:.3} ms/frame", snap_ms * 1000.0 / f);
    println!("  serialize {:.3} ms/frame", ser_ms * 1000.0 / f);
    println!(
        "  total     {:.3} ms/frame ({:.1} fps)",
        (sim_ms + snap_ms + ser_ms) * 1000.0 / f,
        f / (sim_ms + snap_ms + ser_ms)
    );
    println!("  payload   {payload_bytes} bytes/frame");
}

/// Sprint 5 (tech tasks 1/6): measure the checkpoint save+load time at the
/// target population — the B16 atomic write must not impose a surprising cost
/// on the simulation, and the release gate wants a concrete number.
fn bench_checkpoint(n: usize, frames: u64) {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut exporter = TelemetryExporter::disabled();
    for _ in 0..n {
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
    for _ in 0..(n / 2).max(1) {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_grain(&mut sim, Vector2::new(x, y), 10);
    }
    for _ in 0..frames {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }

    let dir = std::env::temp_dir();
    let path = dir.join("bench_checkpoint.bin");
    let p = path.to_str().unwrap().to_string();

    // Save is on the hot path when used for staged replay — measure it.
    let t0 = Instant::now();
    sim.save_checkpoint(&p).expect("save checkpoint");
    let save_s = t0.elapsed().as_secs_f64();

    let t1 = Instant::now();
    let restored = Simulation::load_checkpoint(&p).expect("load checkpoint");
    let load_s = t1.elapsed().as_secs_f64();
    drop(restored);

    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    println!("{n} agents after {frames} frames:");
    println!("  save  {:.3} ms ({size} bytes)", save_s * 1000.0);
    println!("  load  {:.3} ms", load_s * 1000.0);

    let _ = std::fs::remove_file(&p);
}

fn bench_physics(n: usize, frames: u64) {
    let mut phys = PhysicsWorld::new();
    let mut rng = avian_core::rng::SimRng::from_seed(1);
    for _ in 0..n {
        let x = rng.gen_range(2.0..30.0);
        let y = rng.gen_range(2.0..19.0);
        phys.spawn_agent_body(Vector2::new(x, y), 0.3);
    }
    for _ in 0..300 {
        phys.step();
    }
    let t0 = Instant::now();
    for _ in 0..frames {
        phys.step();
    }
    let dt = t0.elapsed().as_secs_f64();
    let ms_per_frame = dt * 1000.0 / frames as f64;
    println!("physics-only {n} bodies: {ms_per_frame:.3} ms/frame");
}

fn bench_export(n: usize) {
    let frame = |i: usize| TelemetryFrame {
        time_us: i as u64 * 8333,
        frame: i as u32,
        uid: format!("A{:04}-{:06}", 1, i % 1000),
        obs: (0..128).map(|k| ((k + i) % 7) as f32 * 0.1).collect(),
        reward: 0.1,
        alarm_triggered: false,
        sick: false,
        reward_grain: 0.0,
        reward_flocking: 0.0,
        reward_starvation: 0.0,
        reward_captured: 0.0,
        reward_flee_success: 0.0,
        fsm: "Spacer".into(),
        event_labels: vec![],
        next_fsm: String::new(),
    };

    let t0 = Instant::now();
    let mut exporter = TelemetryExporter::new(usize::MAX);
    let dir = std::env::temp_dir();
    let path = dir.join("bench_export.csv");
    exporter.open(&path).expect("open export file");
    for i in 0..n {
        exporter.push(frame(i));
    }
    exporter.finish();
    let dt = t0.elapsed().as_secs_f64();
    let throughput = n as f64 / dt;
    println!("export {n} frames in {dt:.2}s: {throughput:.0} frames/s");
    println!(
        "target 100k < 30s: {}",
        if n >= 100_000 && dt < 30.0 {
            "PASS"
        } else {
            "FAIL"
        }
    );
    let _ = std::fs::remove_file(&path);
}
