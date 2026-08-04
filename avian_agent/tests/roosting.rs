//! Audit 4 §9.5 — Roosting state + flock sentinels.
//!
//! At night (light_level < NIGHT_REST_LIGHT_THRESHOLD) a healthy, well-fed
//! flock roosts in place, but a fixed sentinel fraction (~12%) stays in
//! `Scanning` instead. The sentinel assignment is drawn per-tick from `sim.rng`,
//! so it is deterministic for a fixed seed but NOT tied to agent identity.
//! These are statistical tests with fixed seeds: the observed sentinel fraction
//! must sit inside the binomial band around `SENTINEL_FRACTION`, and it must be
//! neither zero nor the whole flock.

use avian_agent::systems::run_systems;
use avian_core::calibration;
use avian_core::components::{Age, FSMState, FeatherCondition, Metabolism};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

const POPULATION: usize = 30;

fn night_sim(seed: u64) -> Simulation {
    let config = SimulationConfig {
        immigration_enabled: false,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(seed, config);
    // Forced midnight — light_level ≈ 0.1 (< 0.3) for the whole test window.
    sim.environment.time_of_day_hours = 0.0;
    for i in 0..POPULATION {
        let x = 4.0 + (i % 10) as f64 * 0.9;
        let y = 4.0 + (i / 10) as f64 * 0.9;
        let uid = sim.next_uid_str();
        let e = avian_agent::gerontology::spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
        // Deterministic control inputs: young/healthy + well-fed so no bird is
        // sick or below CriticalEnergy (which would override Roosting).
        sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
        sim.world.get::<&mut Metabolism>(e).unwrap().energy_kj = 60.0;
        sim.world.get::<&mut FeatherCondition>(e).unwrap().0 = 1.0;
    }
    sim
}

fn count_states(sim: &Simulation) -> (usize, usize) {
    let mut roosting = 0;
    let mut scanning = 0;
    for (_e, fsm) in sim.world.query::<&FSMState>().iter() {
        match *fsm {
            FSMState::Roosting => roosting += 1,
            FSMState::Scanning => scanning += 1,
            _ => {}
        }
    }
    (roosting, scanning)
}

#[test]
fn night_flock_roosts_with_sentinel_fraction() {
    let mut sim = night_sim(7);
    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..10 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    let (roosting, scanning) = count_states(&sim);
    let total = roosting + scanning;
    assert_eq!(
        total, POPULATION,
        "every eligible bird must be Roosting or Scanning"
    );
    let frac = scanning as f64 / POPULATION as f64;
    // Binomial(p = SENTINEL_FRACTION ≈ 0.12, n = 30): P(X=0) ≈ 0.022 and
    // P(X≥10) ≈ 0.005, so a fixed-seed run must land in 1..=10 — neither zero
    // sentinels nor an empty roost — and sit near the target fraction.
    assert!(
        (1..=10).contains(&scanning),
        "sentinel count {scanning} out of the binomial band for n={POPULATION}, p={}",
        calibration::SENTINEL_FRACTION
    );
    assert!(roosting >= 20, "the majority of the flock must be Roosting");
    assert!(
        (frac - calibration::SENTINEL_FRACTION).abs() <= 0.2,
        "sentinel fraction {frac:.3} too far from target {}",
        calibration::SENTINEL_FRACTION
    );
}

#[test]
fn daytime_never_roosts_or_sentinels() {
    // Noon: light ≈ 1.0 → the Roosting branch must never fire.
    let mut sim = Simulation::new(7, SimulationConfig::default());
    sim.environment.time_of_day_hours = 12.0;
    let uid = sim.next_uid_str();
    let e = avian_agent::gerontology::spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        Vector2::new(8.0, 8.0),
        &mut sim.physics,
        uid,
    );
    sim.world.get::<&mut Metabolism>(e).unwrap().energy_kj = 60.0;
    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..10 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let fsm = *sim.world.get::<&FSMState>(e).unwrap();
        assert_ne!(fsm, FSMState::Roosting, "a noon bird must not roost");
        assert_ne!(
            fsm,
            FSMState::Scanning,
            "a noon bird must not be a sentinel"
        );
    }
}

fn sentinel_set(seed: u64) -> Vec<bool> {
    let mut sim = night_sim(seed);
    let mut exporter = TelemetryExporter::new(usize::MAX);
    for _ in 0..3 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    // hecs QueryBorrow has a Drop impl — bind to a let so it drops before `sim`.
    let out: Vec<bool> = sim
        .world
        .query::<&FSMState>()
        .iter()
        .map(|(_e, fsm)| *fsm == FSMState::Scanning)
        .collect();
    out
}

#[test]
fn sentinel_assignment_is_seed_dependent_not_identity_based() {
    // The sentinel draw must come from sim.rng, not from the agent's uid: two
    // different seeds should pick different sentinel subsets of the flock.
    let a = sentinel_set(7);
    let b = sentinel_set(99);
    assert_eq!(a.len(), POPULATION);
    let differing = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    assert!(
        differing > 0,
        "two seeds produced identical sentinel sets — assignment looks identity-based"
    );
}
