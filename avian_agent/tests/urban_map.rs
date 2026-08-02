//! 4.3 Urban map tests: obstacles are plain data + physics colliders, they
//! show up in snapshots, spawn/patrol sampling avoids them, and a building
//! blocks a foraging bird from reaching grain on the far side.

use avian_core::calibration;
use avian_core::components::{Age, Grain, MemorySlot, MemorySlots, ObstacleKind};
use avian_core::{Simulation, SimulationConfig};
use avian_agent::perception::cone_cast;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_telemetry::exporter::TelemetryExporter;
use hecs::Entity;
use nalgebra::Vector2;

/// The default urban map must be built when `config.urban_obstacles` is on,
/// and every obstacle must surface in the snapshot.
#[test]
fn urban_map_builds_obstacles_and_snapshots_them() {
    let mut config = SimulationConfig::default();
    config.urban_obstacles = true;
    let sim = Simulation::new(42, config);

    assert_eq!(sim.obstacles.len(), 5, "default urban map should have 5 obstacles");
    let kinds: Vec<ObstacleKind> = sim.obstacles.iter().map(|o| o.kind).collect();
    assert!(kinds.contains(&ObstacleKind::Building), "map should include buildings");
    assert!(kinds.contains(&ObstacleKind::Tree), "map should include trees");
    assert!(kinds.contains(&ObstacleKind::Water), "map should include water");

    let snap = sim.snapshot();
    assert_eq!(snap.obstacles.len(), 5, "snapshot must carry the obstacles");
    let first = snap.obstacles[0];
    assert_eq!(first.id, 0);
    assert_eq!(first.min[0], 6.0);
    assert_eq!(first.max[0], 10.0);
}

/// With the flag off the arena stays empty (the existing deterministic test
/// scenarios keep their exact trajectories).
#[test]
fn empty_arena_by_default() {
    let sim = Simulation::new(42, SimulationConfig::default());
    assert!(sim.obstacles.is_empty(), "default config must not add obstacles");
    assert!(sim.snapshot().obstacles.is_empty());
}

/// Spawn/patrol sampling must never hand out a point inside an obstacle.
#[test]
fn random_free_point_avoids_obstacles() {
    let mut config = SimulationConfig::default();
    config.urban_obstacles = true;
    let mut sim = Simulation::new(7, config);

    for _ in 0..200 {
        let p = Simulation::random_free_point(
            sim.config.world_width,
            sim.config.world_height,
            &sim.obstacles,
            &mut sim.rng,
        );
        assert!(
            !Simulation::point_in_obstacles(&sim.obstacles, p),
            "free point {p:?} landed inside an obstacle"
        );
        assert!(p.x >= 2.0 && p.x <= 30.0 && p.y >= 2.0 && p.y <= 19.0);
    }

    // The membership predicate itself is exact.
    assert!(Simulation::point_in_obstacles(&sim.obstacles, Vector2::new(8.0, 5.0)));
    assert!(!Simulation::point_in_obstacles(&sim.obstacles, Vector2::new(12.0, 5.0)));
}

/// The perception cone hides a target when the occlusion predicate fires.
#[test]
fn cone_cast_occlusion_filter_hides_target() {
    let origin = Vector2::new(0.0, 0.0);
    let heading = 0.0; // facing +x
    let mut world = hecs::World::new();
    let target = world.spawn(());
    let targets = vec![(target, Vector2::new(5.0, 0.0))];

    let no_block = |_: &Vector2<f64>, _: f64| false;
    let visible = cone_cast(origin, heading, 340.0, 10.0, &targets, no_block);
    assert_eq!(visible.len(), 1, "clear LOS keeps the target");

    let block = |_: &Vector2<f64>, _: f64| true;
    let hidden = cone_cast(origin, heading, 340.0, 10.0, &targets, block);
    assert!(hidden.is_empty(), "occluded target must be hidden");
}

/// Shared scenario: a starved, memory-driven bird (4.2) beelines for a grain
/// patch 15 m away. A full-height building on the route makes the patch
/// unreachable, so the bird never eats; without it the bird arrives (~1450
/// frames). This exercises obstacle colliders AND vision occlusion end-to-end.
fn memory_bird_reaches_grain(building: bool, seed: u64, frames: u64) -> bool {
    let mut config = SimulationConfig::default();
    config.immigration_enabled = false; // deterministic single bird
    let mut sim = Simulation::new(seed, config);
    let uid = sim.next_uid_str();
    let e: Entity = avian_agent::gerontology::spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        Vector2::new(5.0, 5.0),
        &mut sim.physics,
        uid,
    );
    let mut meta = sim.world.get::<&mut avian_core::components::Metabolism>(e).unwrap();
    meta.energy_kj = 4.0;
    meta.crop_count = 0;
    meta.hunger = 0.9;
    drop(meta);
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;

    let patch = Vector2::new(20.0, 5.0);
    spawn_grain(&mut sim, patch, 100);
    sim.world
        .insert(
            e,
            (MemorySlots {
                slots: vec![MemorySlot {
                    pos: patch,
                    strength: 1.0,
                    ttl_frames: calibration::MEMORY_DECAY_FRAMES,
                }],
            },),
        )
        .unwrap();

    if building {
        // Full-height wall splitting the arena — the patch is unreachable.
        sim.add_obstacle(
            ObstacleKind::Building,
            Vector2::new(14.0, 0.0),
            Vector2::new(17.0, 21.0),
        );
    }

    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..frames {
        let before: u32 = sim
            .world
            .query::<&Grain>()
            .iter()
            .map(|(_, g)| g.amount)
            .sum();
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let after: u32 = sim
            .world
            .query::<&Grain>()
            .iter()
            .map(|(_, g)| g.amount)
            .sum();
        if after < before {
            return true;
        }
    }
    false
}

#[test]
fn building_blocks_reaching_far_grain() {
    // Deterministic memory bird: without the building it arrives well inside
    // 3000 frames; with the building it can never get through.
    assert!(
        memory_bird_reaches_grain(false, 1, 3000),
        "control bird should reach the grain"
    );
    assert!(
        !memory_bird_reaches_grain(true, 1, 3000),
        "a building between the bird and the patch must make it unreachable"
    );
}

/// Obstacles survive a checkpoint round-trip (they are plain data, so they
/// travel in the checkpoint rather than a component column).
#[test]
fn obstacles_survive_checkpoint_round_trip() {
    let mut config = SimulationConfig::default();
    config.urban_obstacles = true;
    let sim = Simulation::new(9, config);
    let path = std::env::temp_dir().join("avian_4_3_ckpt.bin");
    let path_str = path.to_str().unwrap().to_string();
    sim.save_checkpoint(&path_str).unwrap();
    let loaded = Simulation::load_checkpoint(&path_str).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(loaded.obstacles.len(), sim.obstacles.len());
    let orig: Vec<(ObstacleKind, [f64; 2], [f64; 2])> = sim
        .obstacles
        .iter()
        .map(|o| (o.kind, [o.min.x, o.min.y], [o.max.x, o.max.y]))
        .collect();
    let loaded_vals: Vec<(ObstacleKind, [f64; 2], [f64; 2])> = loaded
        .obstacles
        .iter()
        .map(|o| (o.kind, [o.min.x, o.min.y], [o.max.x, o.max.y]))
        .collect();
    assert_eq!(orig, loaded_vals, "checkpoint must round-trip the obstacles");
}
