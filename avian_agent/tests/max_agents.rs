//! Sprint 2 (Audit 5): `max_agents` is a hard cap on the live population.
//! Every spawn path that can grow the flock (initial population, immigration)
//! must never push the live count over the configured limit.

use avian_agent::systems::run_systems;
use avian_core::{Simulation, SimulationConfig};

/// Immigration tops the flock back up to `MIN_POPULATION` when it dips below
/// it. With `max_agents` set BELOW `MIN_POPULATION`, the respawn loop must be
/// clamped to the cap — it must never overshoot `max_agents`.
#[test]
fn immigration_never_exceeds_max_agents() {
    let config = SimulationConfig {
        max_agents: 5,
        immigration_enabled: true,
        initial_agents: 3,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(11, config);

    let live = |sim: &Simulation| {
        sim.world
            .query::<&avian_core::components::Metabolism>()
            .iter()
            .count()
    };

    // With immigration on, the population should be topped up toward
    // MIN_POPULATION (10) — but hard-capped at max_agents (5).
    assert!(
        live(&sim) <= 5,
        "initial population must respect max_agents"
    );

    for _ in 0..500 {
        sim.step(run_systems);
        assert!(
            live(&sim) <= 5,
            "live population exceeded max_agents=5: {}",
            live(&sim)
        );
    }
}

/// Immigration must also work normally when `max_agents` is comfortably above
/// `MIN_POPULATION` (the common case).
#[test]
fn immigration_reaches_min_population_when_cap_is_high() {
    let config = SimulationConfig {
        max_agents: 1000,
        immigration_enabled: true,
        initial_agents: 1,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(12, config);

    for _ in 0..200 {
        sim.step(run_systems);
    }
    let live = sim
        .world
        .query::<&avian_core::components::Metabolism>()
        .iter()
        .count();
    assert!(
        live <= 1000,
        "population must never exceed max_agents, got {live}"
    );
    // Immigration fills toward MIN_POPULATION; with no deaths, it should be
    // there after a couple hundred frames.
    assert!(live >= avian_core::calibration::MIN_POPULATION);
}
