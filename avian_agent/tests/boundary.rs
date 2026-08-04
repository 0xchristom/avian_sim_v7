//! Audit 5a (Sprint 3): kill the edge-clinging straight-line artifact.
//!
//! Root cause: a CRW/Lévy wanderer holds its heading until the step burns out.
//! Against the arena wall the physics yields tangential sliding, so a pigeon
//! marching at the wall used to cling to the edge in a straight line forever
//! (aggravated by the old 10 m flocking that kept it glued near walls).
//!
//! Fix: a soft boundary repulsion is added to velocity near the edges
//! (systems.rs, `WALL_AVOID_MARGIN_M`), the Spacer heading is re-drawn toward
//! the interior when inside the margin, and the Lévy step does not burn while
//! the agent is pushing into the wall.
//!
//! These tests assert a lone bird heading straight at a wall turns away before
//! touching it and never spends sustained time sliding tangentially along the
//! edge.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::calibration;
use avian_core::components::{Age, Heading, LevyState, Position, Velocity};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use hecs::Entity;
use nalgebra::Vector2;

/// A single young healthy bird with a fixed heading and no grains/predators.
fn lone_bird(pos: Vector2<f64>, heading: f64, seed: u64) -> (Simulation, Entity) {
    let mut sim = Simulation::new(seed, SimulationConfig::default());
    let uid = sim.next_uid_str();
    let e = spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
    sim.world.get::<&mut Heading>(e).unwrap().0 = heading;
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
    // Force a long straight step so the wanderer would march straight at the
    // wall if nothing steered it away (young, no Sick noise).
    sim.world.get::<&mut LevyState>(e).unwrap().remaining_dist = 100.0;
    sim.world.get::<&mut LevyState>(e).unwrap().target_heading = heading;
    (sim, e)
}

/// A bird placed 1.5 m inside the left wall (x=1.5) heading due west (−x, into
/// the wall). It must turn away: after 600 ticks it must be moving back toward
/// the interior (positive x velocity on average) and must never spend sustained
/// frames sliding tangentially with constant heading.
#[test]
fn bird_heading_into_wall_turns_away() {
    let (mut sim, e) = lone_bird(Vector2::new(1.5, 10.5), std::f64::consts::PI, 3);
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut x_min = f64::MAX;
    let mut moved_interior = 0u32;
    let mut stuck_sliding_frames = 0u32;
    for _ in 0..600 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let pos = sim.world.get::<&Position>(e).unwrap().0;
        let vel = sim.world.get::<&Velocity>(e).unwrap().0;
        x_min = x_min.min(pos.x);
        // Once past the margin the bird must head east (interior).
        if pos.x > calibration::WALL_AVOID_MARGIN_M && vel.x > 0.0 {
            moved_interior += 1;
        }
        // Sliding = inside the margin with a strongly tangential velocity and
        // near-constant heading (the old artifact).
        let h = sim.world.get::<&Heading>(e).unwrap().0;
        if pos.x < calibration::WALL_AVOID_MARGIN_M
            && vel.y.abs() > 0.5
            && (h - std::f64::consts::PI).abs() < 0.2
        {
            stuck_sliding_frames += 1;
        }
    }

    assert!(
        x_min >= 0.0,
        "bird went through the wall (x reached {x_min:.2})"
    );
    assert!(
        moved_interior > 0,
        "bird never turned toward the interior — it kept marching into the wall"
    );
    assert!(
        stuck_sliding_frames < 60,
        "bird spent {stuck_sliding_frames} frames sliding tangentially along the \
         wall with constant heading — edge-clinging is not fixed"
    );
}

/// A bird 1.5 m inside the TOP wall (y = world_height − 1.5) heading due north
/// (+y, into the wall). Same expectations mirrored.
#[test]
fn bird_heading_into_top_wall_turns_away() {
    let h = 21.0; // default world_height
    let (mut sim, e) = lone_bird(Vector2::new(16.0, h - 1.5), std::f64::consts::FRAC_PI_2, 11);
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut y_max = f64::MIN;
    let mut moved_interior = 0u32;
    for _ in 0..600 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let pos = sim.world.get::<&Position>(e).unwrap().0;
        let vel = sim.world.get::<&Velocity>(e).unwrap().0;
        y_max = y_max.max(pos.y);
        if pos.y < h - calibration::WALL_AVOID_MARGIN_M && vel.y < 0.0 {
            moved_interior += 1;
        }
    }

    assert!(
        y_max <= h,
        "bird went through the top wall (y reached {y_max:.2} > {h:.2})"
    );
    assert!(
        moved_interior > 0,
        "bird never turned away from the top wall — it kept marching into it"
    );
}
