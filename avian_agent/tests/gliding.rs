//! Phase 9 (Audit 3) — emergent aerodynamics: building thermals + Gliding FSM.
//!
//! Validates that:
//! 1. Thermal updraft zones form on the SUN-FACING side of `ObstacleKind::Building`
//!    and that the zone follows the day/night sun position.
//! 2. A bird inside a thermal whose heading aligns with the updraft `flow`
//!    vector enters `FSMState::Gliding`.
//! 3. A bird far from any thermal never glides.
//! 4. The glide MR multiplier is near-zero vs active flight (unit-level).
//! 5. Gliding actually drains less energy than ground locomotion.

use avian_agent::systems::run_systems;
use avian_core::calibration;
use avian_core::components::{Age, FSMState, FeatherCondition, Heading, Position, Velocity};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

fn urban_sim(seed: u64) -> Simulation {
    let mut config = SimulationConfig::default();
    config.urban_obstacles = true;
    config.immigration_enabled = false;
    Simulation::new(seed, config)
}

fn spawn_controlled_agent(sim: &mut Simulation, pos: Vector2<f64>, heading: f64) -> hecs::Entity {
    let uid = sim.next_uid_str();
    let e = avian_agent::gerontology::spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        pos,
        &mut sim.physics,
        uid,
    );
    // Deterministic control inputs: young/healthy, well-fed, pristine feathers,
    // explicit heading — so the ONLY difference between test arms is the
    // thermal/heading alignment.
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
    sim.world.get::<&mut Heading>(e).unwrap().0 = heading;
    sim.world.get::<&mut Position>(e).unwrap().0 = pos;
    sim.world.get::<&mut FeatherCondition>(e).unwrap().0 = 1.0;
    e
}

/// Default urban map (Simulation::build_default_obstacles): two buildings —
/// A=[6,3]-[10,7] and B=[16,8]-[21,11].
const BUILDING_A: ([f64; 2], [f64; 2]) = ([6.0, 3.0], [10.0, 7.0]);

#[test]
fn thermal_zones_form_on_sun_facing_building_sides() {
    let mut sim = urban_sim(42);

    // Noon (12h): sun_heading = -(12-6)/12·π = -π/2 → sun from the SOUTH.
    // Building A's south face (y=3) must host the updraft strip.
    sim.step(|s, _dt| {
        let _ = s;
    });
    sim.update_thermals();
    assert_eq!(sim.thermals.len(), 2, "two buildings → two thermal zones");
    let (amin, amax) = BUILDING_A;
    let south = sim.thermals.iter().find(|t| {
        (t.min.y - (amin[1] - calibration::THERMAL_DEPTH_M)).abs() < 1e-9
            && (t.max.y - amin[1]).abs() < 1e-9
            && (t.min.x - amin[0]).abs() < 1e-9
            && (t.max.x - amax[0]).abs() < 1e-9
    });
    assert!(
        south.is_some(),
        "noon: thermal must be on the SOUTH face of building A"
    );
    assert_eq!(
        south.unwrap().flow,
        Vector2::new(-1.0, 0.0),
        "south-face updraft flows -x"
    );

    // Sunrise (6h): sun_heading = 0 → sun from the EAST → EAST face hosts it.
    sim.environment.time_of_day_hours = 6.0;
    sim.environment.sun_heading = -(6.0 - 6.0) / 12.0 * std::f64::consts::PI;
    sim.update_thermals();
    let east = sim.thermals.iter().find(|t| {
        (t.min.x - amax[0]).abs() < 1e-9
            && (t.max.x - (amax[0] + calibration::THERMAL_DEPTH_M)).abs() < 1e-9
            && (t.min.y - amin[1]).abs() < 1e-9
            && (t.max.y - amax[1]).abs() < 1e-9
    });
    assert!(
        east.is_some(),
        "sunrise: thermal must be on the EAST face of building A"
    );
    assert_eq!(
        east.unwrap().flow,
        Vector2::new(0.0, 1.0),
        "east-face updraft rises +y"
    );
}

#[test]
fn glide_state_entered_in_aligned_thermal() {
    let mut sim = urban_sim(7);
    sim.environment.time_of_day_hours = 12.0;
    sim.environment.sun_heading = -std::f64::consts::FRAC_PI_2;

    // South thermal of building A: x∈[6,10], y∈[0.5,3.0], flow = (-1,0)=west.
    // Bird at (8,2) heading west (π) is inside + aligned → glides on frame 1.
    let e = spawn_controlled_agent(&mut sim, Vector2::new(8.0, 2.0), std::f64::consts::PI);
    let mut exporter = TelemetryExporter::new(usize::MAX);

    sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    let fsm = *sim.world.get::<&FSMState>(e).unwrap();
    assert_eq!(
        fsm,
        FSMState::Gliding,
        "aligned bird in thermal must enter Gliding"
    );

    // It must also be genuinely airborne (glide cruise speed).
    let v = sim.world.get::<&Velocity>(e).unwrap().0;
    assert!(
        v.norm() >= calibration::FLIGHT_SPEED_THRESHOLD_MS,
        "gliding bird must be airborne (speed {} < {})",
        v.norm(),
        calibration::FLIGHT_SPEED_THRESHOLD_MS
    );
}

#[test]
fn no_glide_outside_thermal() {
    let mut sim = urban_sim(7);
    sim.environment.time_of_day_hours = 12.0;
    sim.environment.sun_heading = -std::f64::consts::FRAC_PI_2;

    // Far corner (top-right): no thermal anywhere near → must never glide.
    let e = spawn_controlled_agent(&mut sim, Vector2::new(25.0, 18.0), 0.0);
    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..100 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let fsm = *sim.world.get::<&FSMState>(e).unwrap();
        assert_ne!(
            fsm,
            FSMState::Gliding,
            "bird outside any thermal must not glide"
        );
    }
}

#[test]
fn glide_mr_is_near_zero_versus_flight() {
    let v = calibration::GLIDE_SPEED_MS;
    // Active flapping at the same airspeed pays the full 7× flight multiplier…
    assert_eq!(
        calibration::flight_mr_multiplier_state(v, false),
        calibration::FLIGHT_MR_MULTIPLIER
    );
    // …while gliding pays GLIDE_MR_MULTIPLIER (near-zero).
    assert_eq!(
        calibration::flight_mr_multiplier_state(v, true),
        calibration::GLIDE_MR_MULTIPLIER
    );
    assert!(
        calibration::GLIDE_MR_MULTIPLIER < 0.25,
        "glide MR {:.2} must be near-zero",
        calibration::GLIDE_MR_MULTIPLIER
    );
    assert!(
        calibration::GLIDE_MR_MULTIPLIER < calibration::FLIGHT_MR_MULTIPLIER,
        "glide must cost less than active flight"
    );
}

#[test]
fn gliding_drains_less_energy_than_ground_locomotion() {
    // Same seed, same spawn → identical RNG-derived age/mass/energy. The ONLY
    // difference is position/heading: one bird launches into the south thermal
    // (glides), the other just walks a corner far from any building.
    let run_drain = |pos: Vector2<f64>, heading: f64| -> f64 {
        let mut sim = urban_sim(99);
        sim.environment.time_of_day_hours = 12.0;
        sim.environment.sun_heading = -std::f64::consts::FRAC_PI_2;
        spawn_controlled_agent(&mut sim, pos, heading);
        let mut exporter = TelemetryExporter::new(usize::MAX);
        for _ in 0..60 {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        }
        sim.total_energy_expenditure_kj
    };

    let glide = run_drain(Vector2::new(8.0, 2.0), std::f64::consts::PI);
    let walk = run_drain(Vector2::new(25.0, 18.0), 0.0);
    assert!(
        glide < walk,
        "gliding bird drained {glide:.3} kJ — should be less than walking {walk:.3} kJ"
    );
}
