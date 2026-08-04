//! 2.6 acceptance: agents with low feather condition enter Preening and
//! restore their feathers; over a long run the FSM histogram shows a
//! preening share near the ~10% pigeon time budget.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::components::{Age, FSMState, FeatherCondition, Metabolism};
use avian_core::{Simulation, SimulationConfig};
use hecs::Entity;
use nalgebra::Vector2;
use std::collections::HashMap;

#[test]
fn test_low_feathers_triggers_preening_and_restores() {
    let mut sim = Simulation::new(5, SimulationConfig::default());
    let uid = sim.next_uid_str();
    let e = spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        Vector2::new(16.0, 10.5),
        &mut sim.physics,
        uid,
    );
    // Young age → vitality ~0.84, so the 2.7 Sick branch never preempts preening.
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
    sim.world.get::<&mut FeatherCondition>(e).unwrap().0 = 0.1;

    let mut saw_preening = false;
    for _ in 0..120 {
        sim.step(run_systems);
        let fsm = *sim.world.get::<&FSMState>(e).unwrap();
        if fsm == FSMState::Preening {
            saw_preening = true;
        }
    }

    assert!(saw_preening, "low-feather agent never entered Preening");
    let restored = sim.world.get::<&FeatherCondition>(e).unwrap().0;
    assert!(
        restored > 0.3,
        "preening did not restore feathers (now {restored:.2})"
    );
}

#[test]
fn test_preening_time_budget_share() {
    let mut sim = Simulation::new(99, SimulationConfig::default());
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        let uid = sim.next_uid_str();
        let e = spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
        // Force young (vitality ~0.84) so the 2.7 Sick branch can't starve the
        // preening duty cycle and skew the share below the band.
        sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
    }

    let mut agent_frames = 0u64;
    let mut preen_frames = 0u64;
    for _ in 0..5000 {
        sim.step(run_systems);
        let mut preening = 0usize;
        let mut total = 0usize;
        for (_, fsm) in sim.world.query::<&FSMState>().iter() {
            total += 1;
            if *fsm == FSMState::Preening {
                preening += 1;
            }
        }
        agent_frames += total as u64;
        preen_frames += preening as u64;
    }

    let share = preen_frames as f64 / agent_frames as f64;
    assert!(
        (0.03..=0.20).contains(&share),
        "preening share {:.1}% outside the ~10% time-budget band (3-20%)",
        share * 100.0
    );
}

// Audit 5a item 1 regression: with a per-agent INITIAL feather value sampled
// from the seeded RNG (0.6–1.0) instead of a flat constant, birds cross
// PREEN_FEATHER_THRESHOLD on DIFFERENT ticks — not the same frame. This test
// checks the actual failure mode: it counts the distinct ticks at which agents
// FIRST enter Preening and requires more than one. (It is not enough to assert
// "some birds are preening" — that would pass even when they all start
// together.)
#[test]
fn test_preening_is_desynchronized_across_agents() {
    // Immigration off keeps the population at exactly what we spawn, so the
    // set of "first preening ticks" is stable for the whole window.
    let config = SimulationConfig {
        immigration_enabled: false,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(11, config);
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        let uid = sim.next_uid_str();
        let e = spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
        // Young + well-fed: the 2.7 Sick branch and starvation must not
        // preempt the feather-driven Preening transition.
        sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
        sim.world.get::<&mut Metabolism>(e).unwrap().energy_kj = 60.0;
    }

    // First tick (frame) each agent enters Preening.
    let mut first_preen: HashMap<Entity, u64> = HashMap::new();
    for tick in 0..4000u64 {
        sim.step(run_systems);
        for (e, fsm) in sim.world.query::<&FSMState>().iter() {
            if *fsm == FSMState::Preening && !first_preen.contains_key(&e) {
                first_preen.insert(e, tick);
            }
        }
    }

    assert_eq!(
        first_preen.len(),
        30,
        "every agent should have entered Preening within the window"
    );
    let distinct_ticks: std::collections::HashSet<u64> = first_preen.values().copied().collect();
    assert!(
        distinct_ticks.len() > 1,
        "all {first_preen:?} agents entered Preening on the same tick — \
         synchronized preening not fixed"
    );
}

// Regression: preening must stay desynchronized ACROSS a night. The old decay
// ran unconditionally while Roost (higher priority than Preen) blocked the
// restore, so every bird's feathers clamped to 0 overnight and ALL agents
// entered Preening together at dawn, permanently locking them in phase.
// No per-tick must ever have the whole population preening simultaneously.
#[test]
fn test_preening_stays_desynchronized_across_night() {
    // Short day + dt = 1 s/frame so a full night fits in a fast window:
    // day_length 100 s → night (light < 0.3) spans roughly sim-seconds 34→66
    // from a noon start; 120 frames covers it end to end. All rates are per
    // sim-second, so the dt choice is behaviourally invariant.
    let config = SimulationConfig {
        immigration_enabled: false,
        day_length_sim_s: 100.0,
        dt: 1.0,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(11, config);
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        let uid = sim.next_uid_str();
        let e = spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(x, y),
            &mut sim.physics,
            uid,
        );
        // Young + well-fed so Sick/starvation never preempts Preening.
        sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
        sim.world.get::<&mut Metabolism>(e).unwrap().energy_kj = 60.0;
    }

    let mut max_simultaneous = 0usize;
    let mut saw_night = false;
    for _ in 0..120u64 {
        sim.step(run_systems);
        if sim.environment.light_level < avian_core::calibration::NIGHT_REST_LIGHT_THRESHOLD {
            saw_night = true;
        }
        let mut preening = 0usize;
        for (_e, fsm) in sim.world.query::<&FSMState>().iter() {
            if *fsm == FSMState::Preening {
                preening += 1;
            }
        }
        max_simultaneous = max_simultaneous.max(preening);
    }

    assert!(
        saw_night,
        "test never reached night — day_length too long for the window"
    );
    assert!(
        max_simultaneous < 30,
        "whole population preened simultaneously ({max_simultaneous} of 30) — \
         night reset every bird's feather phase; preening is synchronized"
    );
}
