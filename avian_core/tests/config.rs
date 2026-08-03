//! 5.2 Config File System: `simulation.toml` carries scenario params only
//! (world dims, obstacle layout, initial population, seed, pacing, behavior
//! toggles, event schedule). Biology constants stay in `calibration.rs` — the
//! defaults here must equal the compiled constants so an empty file is
//! indistinguishable from `SimulationConfig::default()`.

use avian_core::components::ObstacleKind;
use avian_core::events::Event;
use avian_core::{Simulation, SimulationConfig};
use nalgebra::Vector2;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn write_temp_toml(name: &str, body: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "avian_config_{}_{}_{}.toml",
        name,
        std::process::id(),
        n
    ));
    std::fs::write(&path, body).unwrap();
    path
}

/// A full scenario file parses every documented 5.2 scenario param.
#[test]
fn from_file_parses_full_scenario() {
    let path = write_temp_toml(
        "full",
        r#"
seed = 1234
world_width = 40.0
world_height = 25.0
initial_agents = 50
initial_grains = 20
time_scale = 2.0
foraging_threshold = 0.55
flocking_enabled = false
max_agents = 2000
weather_enabled = true

[[obstacles]]
kind = "Building"
min = [6.0, 3.0]
max = [10.0, 7.0]

[[obstacles]]
kind = "Water"
min = [7.0, 12.0]
max = [11.0, 13.5]

[[event_schedule]]
event = "spawn_grain"
pos = [5.0, 5.0]
count = 3

[[event_schedule]]
event = "set_weather"
weather = "Rain"
"#,
    );

    let cfg = SimulationConfig::from_file(path.to_str().unwrap()).unwrap();
    assert_eq!(cfg.seed, Some(1234));
    assert_eq!(cfg.world_width, 40.0);
    assert_eq!(cfg.world_height, 25.0);
    assert_eq!(cfg.initial_agents, 50);
    assert_eq!(cfg.initial_grains, 20);
    assert_eq!(cfg.time_scale, 2.0);
    assert_eq!(cfg.foraging_threshold, 0.55);
    assert!(!cfg.flocking_enabled);
    assert_eq!(cfg.max_agents, 2000);
    assert!(cfg.weather_enabled);

    assert_eq!(cfg.obstacles.len(), 2);
    assert_eq!(cfg.obstacles[0].kind, ObstacleKind::Building);
    assert_eq!(cfg.obstacles[1].kind, ObstacleKind::Water);

    assert_eq!(cfg.event_schedule.len(), 2);
    match &cfg.event_schedule[0] {
        Event::SpawnGrain(req) => {
            assert_eq!(req.pos, [5.0, 5.0]);
            assert_eq!(req.count, 3);
        }
        other => panic!("expected SpawnGrain, got {other:?}"),
    }
    match &cfg.event_schedule[1] {
        Event::SetWeather(req) => assert_eq!(req.weather, avian_core::components::Weather::Rain),
        other => panic!("expected SetWeather, got {other:?}"),
    }
}

/// An omitted field falls back to the compiled default — a partial file is
/// still valid, and biology constants are never redefined by the file.
#[test]
fn from_file_omitted_fields_fill_defaults() {
    let path = write_temp_toml("partial", "max_agents = 500\n");
    let cfg = SimulationConfig::from_file(path.to_str().unwrap()).unwrap();
    let d = SimulationConfig::default();
    assert_eq!(cfg.max_agents, 500);
    assert_eq!(cfg.seed, d.seed);
    assert_eq!(cfg.world_width, d.world_width);
    assert_eq!(cfg.world_height, d.world_height);
    assert_eq!(cfg.initial_agents, d.initial_agents);
    assert_eq!(cfg.initial_grains, d.initial_grains);
    assert_eq!(cfg.time_scale, d.time_scale);
    assert_eq!(cfg.foraging_threshold, d.foraging_threshold);
    assert!(cfg.flocking_enabled);
    assert!(!cfg.urban_obstacles);
    assert!(!cfg.weather_enabled);
    assert!(cfg.obstacles.is_empty());
    assert!(cfg.event_schedule.is_empty());
}

/// Default config round-trips through toml losslessly.
#[test]
fn default_config_roundtrips() {
    let d = SimulationConfig::default();
    let s = d.to_toml_string().unwrap();
    let back: SimulationConfig = toml::from_str(&s).unwrap();
    assert_eq!(back.dt, d.dt);
    assert_eq!(back.gravity, d.gravity);
    assert_eq!(back.max_agents, d.max_agents);
    assert_eq!(back.predator_expiry, d.predator_expiry);
    assert_eq!(back.immigration_enabled, d.immigration_enabled);
    assert_eq!(back.seed, d.seed);
    assert_eq!(back.world_width, d.world_width);
    assert_eq!(back.world_height, d.world_height);
    assert_eq!(back.initial_agents, d.initial_agents);
    assert_eq!(back.initial_grains, d.initial_grains);
    assert_eq!(back.time_scale, d.time_scale);
    assert_eq!(back.foraging_threshold, d.foraging_threshold);
    assert_eq!(back.flocking_enabled, d.flocking_enabled);
}

/// A custom obstacle layout in the file materializes into physics colliders
/// that block line-of-sight exactly like the 4.3 built-in map.
#[test]
fn custom_obstacle_layout_blocks_los() {
    let mut cfg = SimulationConfig::default();
    cfg.obstacles = vec![avian_core::ObstacleSpec {
        kind: ObstacleKind::Building,
        min: [10.0, 4.0],
        max: [14.0, 8.0],
    }];
    let sim = Simulation::new(42, cfg);

    assert_eq!(sim.obstacles.len(), 1);
    assert_eq!(sim.obstacles[0].kind, ObstacleKind::Building);

    // Origin west of the building, looking east into it.
    let hit = sim
        .physics
        .cast_ray_to_static(Vector2::new(9.0, 6.0), Vector2::new(1.0, 0.0), 10.0);
    let toi = hit.expect("building must occlude the ray");
    assert!(
        (toi - 1.0).abs() < 1e-3,
        "building spans x 10..14, hit at {toi}"
    );
}

/// World dimensions drive the wall placement (here a 10×8 arena) — the top
/// wall is cast-able at y=8.
#[test]
fn world_dimensions_place_walls() {
    let mut cfg = SimulationConfig::default();
    cfg.world_width = 10.0;
    cfg.world_height = 8.0;
    let sim = Simulation::new(1, cfg);

    // Standing below the top wall, looking straight up: must hit at y=8.
    let toi = sim
        .physics
        .cast_ray_to_static(Vector2::new(5.0, 7.5), Vector2::new(0.0, 1.0), 1.0)
        .expect("top wall should exist at y=8");
    assert!(
        (toi - 0.5).abs() < 1e-3,
        "distance 0.5 up → toi 0.5, got {toi}"
    );

    // From a point well inside a 32×21 arena this wall would NOT be in range
    // (sanity: the default arena keeps its 21 m height).
    let def = Simulation::new(1, SimulationConfig::default());
    assert!(
        def.physics
            .cast_ray_to_static(Vector2::new(5.0, 7.5), Vector2::new(0.0, 1.0), 1.0)
            .is_none(),
        "default arena's top wall is >1 m above y=7.5"
    );
}

/// `from_config` uses the config's `seed` field (falling back to 42).
#[test]
fn from_config_uses_seed_field() {
    let mut cfg = SimulationConfig::default();
    cfg.seed = Some(7);
    let mut a = Simulation::from_config(cfg.clone());
    let mut b = Simulation::new(7, cfg);
    // Same seed → identical RNG stream.
    assert_eq!(
        a.rng.gen_range(0u64..1_000_000),
        b.rng.gen_range(0u64..1_000_000)
    );

    let no_seed = SimulationConfig::default();
    let mut c = Simulation::from_config(no_seed.clone());
    let mut d = Simulation::new(42, no_seed);
    assert_eq!(
        c.rng.gen_range(0u64..1_000_000),
        d.rng.gen_range(0u64..1_000_000)
    );
}

// Sprint 1 (Audit 5): SimulationConfig::validate() rejects the config
// invariants the simulation depends on, and the constructors refuse to build a
// broken simulation.

#[test]
fn validate_rejects_zero_dt() {
    let mut cfg = SimulationConfig::default();
    cfg.dt = 0.0;
    assert!(cfg.validate().is_err(), "dt == 0 must be rejected");
    assert!(Simulation::try_new(1, cfg.clone()).is_err());
    assert!(Simulation::try_from_config(cfg).is_err());
}

#[test]
fn validate_rejects_nan_and_infinite_dt() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut cfg = SimulationConfig::default();
        cfg.dt = bad;
        assert!(cfg.validate().is_err(), "dt={bad} must be rejected");
    }
}

#[test]
fn validate_rejects_bad_world_dimensions() {
    for (w, h) in [(0.0, 21.0), (32.0, 0.0), (-5.0, 21.0), (f64::NAN, 21.0)] {
        let mut cfg = SimulationConfig::default();
        cfg.world_width = w;
        cfg.world_height = h;
        assert!(cfg.validate().is_err(), "world {w}x{h} must be rejected");
    }
}

#[test]
fn validate_rejects_bad_time_scale_and_day_length() {
    let mut cfg = SimulationConfig::default();
    cfg.time_scale = 0.0;
    assert!(cfg.validate().is_err());
    let mut cfg = SimulationConfig::default();
    cfg.day_length_sim_s = 0.0;
    assert!(cfg.validate().is_err());
    let mut cfg = SimulationConfig::default();
    cfg.day_length_sim_s = f64::NAN;
    assert!(cfg.validate().is_err());
}

#[test]
fn validate_rejects_zero_max_agents_and_overspawn() {
    let mut cfg = SimulationConfig::default();
    cfg.max_agents = 0;
    assert!(cfg.validate().is_err());
    let mut cfg = SimulationConfig::default();
    cfg.max_agents = 10;
    cfg.initial_agents = 11;
    assert!(
        cfg.validate().is_err(),
        "initial_agents > max_agents must fail"
    );
}

#[test]
fn validate_rejects_bad_obstacle_boxes() {
    // Reversed box (max < min).
    let mut cfg = SimulationConfig::default();
    cfg.obstacles = vec![avian_core::ObstacleSpec {
        kind: ObstacleKind::Building,
        min: [10.0, 4.0],
        max: [5.0, 8.0],
    }];
    assert!(cfg.validate().is_err(), "reversed box must fail");

    // Box outside the arena.
    let mut cfg = SimulationConfig::default();
    cfg.obstacles = vec![avian_core::ObstacleSpec {
        kind: ObstacleKind::Building,
        min: [30.0, 4.0],
        max: [40.0, 8.0],
    }];
    assert!(cfg.validate().is_err(), "out-of-arena box must fail");
}

#[test]
fn validate_accepts_default_and_valid_custom() {
    assert!(SimulationConfig::default().validate().is_ok());
    let mut cfg = SimulationConfig::default();
    cfg.dt = 1.0 / 60.0;
    cfg.gravity = -9.81;
    cfg.world_width = 40.0;
    cfg.world_height = 25.0;
    cfg.max_agents = 500;
    cfg.initial_agents = 50;
    assert!(cfg.validate().is_ok());
    let sim = Simulation::try_new(1, cfg.clone()).expect("valid config builds");
    assert!(
        (sim.physics.dt() - 1.0 / 60.0).abs() < 1e-9,
        "physics must use config dt"
    );
    assert_eq!(sim.physics.gravity.y, -9.81, "gravity must reach Rapier");
}

/// Sprint 1: non-default dt + gravity change the integration parameters and
/// body behavior — the physics world is driven by config, not hard-coded.
#[test]
fn physics_uses_config_dt_and_gravity() {
    let mut cfg = SimulationConfig::default();
    cfg.dt = 1.0 / 60.0;
    cfg.gravity = -9.81;
    let mut sim = Simulation::try_new(1, cfg).expect("valid config");
    assert!((sim.physics.dt() - 1.0 / 60.0).abs() < 1e-9);

    // A dynamic body must fall under non-zero gravity after one solver step.
    let h = sim.physics.spawn_agent_body(Vector2::new(16.0, 16.0), 0.3);
    sim.physics.step();
    let rb = sim.physics.get_body(h).unwrap();
    let v = rb.linvel();
    assert!(
        v.y < 0.0,
        "gravity must act on a dynamic body, got vy={}",
        v.y
    );
}

/// Sprint 2 (Audit 5): `random_free_point` must report failure (None) when the
/// arena is so obstacle-dense that no free point can be found — never fall back
/// to a point inside a collider.
#[test]
fn random_free_point_returns_none_when_fully_blocked() {
    let mut sim = Simulation::new(1, SimulationConfig::default());
    // Cover the whole arena with one obstacle so every draw is inside it.
    let blocked = vec![avian_core::components::Obstacle {
        id: 0,
        kind: ObstacleKind::Building,
        min: Vector2::new(0.0, 0.0),
        max: Vector2::new(sim.config.world_width, sim.config.world_height),
    }];
    let p = Simulation::random_free_point(
        sim.config.world_width,
        sim.config.world_height,
        &blocked,
        &mut sim.rng,
    );
    assert!(
        p.is_none(),
        "fully-obstructed arena must yield None, got {:?}",
        p
    );
    // A clear arena still yields Some.
    let p2 = Simulation::random_free_point(
        sim.config.world_width,
        sim.config.world_height,
        &[],
        &mut sim.rng,
    );
    assert!(p2.is_some(), "empty-obstacle arena must yield a point");
}
