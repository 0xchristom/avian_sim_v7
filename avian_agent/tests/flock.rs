//! Audit 5a (Sprint 2): free-roaming pigeons must be able to LEAVE a flock.
//!
//! Root cause of "permanent flocking": boids is a steering FORCE summed onto
//! the tree-selected velocity every tick, and three things made it a
//! self-reinforcing attractor:
//!
//! 1. Neighbors were pulled from the 10 m vision-range k-nearest set instead of
//!    the calibrated `BOID_NEIGHBOR_RADIUS_M` (3 m), so the cohesion field was
//!    >3× larger than documented and any bird within 10 m was captured.
//! 2. `Spacer` (the default free-roam state) kept the full 0.5 cohesion weight —
//!    a wanderer was gravitationally bound to any cluster in range.
//! 3. The summed steering was never clamped, so cohesion (0.5 × distance) could
//!    inject 2.5 m/s onto a wanderer whose own walk speed is 0.96 m/s.
//!
//! These tests assert the fixes hold: two birds 8 m apart are NOT glued
//! together, no non-fleeing velocity exceeds max_speed_ms, and roosting birds
//! get no boids steering.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::components::{FSMState, Mobility, Position, Velocity};
use avian_core::{Simulation, SimulationConfig};
use nalgebra::Vector2;

/// Spawn `n` birds at `x_offsets` meters apart on the same row, no grains, no
/// predators — a pure flock-dynamics scenario.
fn flock_sim(n: usize, x_offsets: &[f64], seed: u64) -> Simulation {
    let mut sim = Simulation::new(seed, SimulationConfig::default());
    for offset in x_offsets.iter().take(n) {
        let uid = sim.next_uid_str();
        spawn_agent(
            &mut sim.world,
            &mut sim.rng,
            Vector2::new(10.0 + *offset, 10.5),
            &mut sim.physics,
            uid,
        );
    }
    sim
}

/// Two birds spawn 8 m apart (well beyond the 3 m boids neighborhood). After a
/// long wander window they must NOT be glued together — they should drift apart
/// and spend most of the run separated, proving the 3 m radius filter plus the
/// reduced Spacer cohesion let a lone wanderer escape. Before the fix the 10 m
/// cohesion field pinned the pair at ~1 m within seconds.
#[test]
fn birds_8m_apart_are_not_glued_together() {
    let mut sim = flock_sim(2, &[0.0, 8.0], 42);

    let mut min_sep = f64::MAX;
    let mut max_sep = f64::MIN;
    let mut sep_sum = 0.0;
    let mut samples = 0u64;
    for _ in 0..2000 {
        sim.step(run_systems);
        let positions: Vec<Vector2<f64>> = sim
            .world
            .query::<&Position>()
            .iter()
            .map(|(_, p)| p.0)
            .collect();
        let d = (positions[0] - positions[1]).norm();
        min_sep = min_sep.min(d);
        max_sep = max_sep.max(d);
        sep_sum += d;
        samples += 1;
    }

    let mean_sep = sep_sum / samples as f64;
    assert!(
        min_sep > 3.0,
        "birds collapsed to {min_sep:.2} m — they got glued together (a \
         wanderer should be able to keep its distance from a cluster)"
    );
    assert!(
        max_sep > 6.0,
        "birds never separated beyond {max_sep:.2} m — they are permanently \
         flocked; the Spacer cohesion / radius filter is not letting a wanderer leave"
    );
    assert!(
        mean_sep > 6.0,
        "mean separation {mean_sep:.2} m too low — pair is effectively glued \
         (expected ~8 m for two birds starting 8 m apart and drifting freely)"
    );
}

/// No non-fleeing velocity may exceed the agent's own max_speed_ms. The clamp
/// was added because boids steering (unclamped) routinely oversped wanderers.
#[test]
fn non_fleeing_velocity_is_clamped_to_max_speed() {
    let mut sim = flock_sim(2, &[0.0, 2.0], 7); // 2 m apart → boids active

    for _ in 0..600 {
        sim.step(run_systems);
        for (_, (fsm, vel, mob)) in sim
            .world
            .query::<(&FSMState, &Velocity, &Mobility)>()
            .iter()
        {
            let speed = vel.0.norm();
            if *fsm != FSMState::Fleeing {
                assert!(
                    speed <= mob.max_speed_ms * 1.05 + 1e-9,
                    "non-fleeing {fsm:?} speed {speed:.3} m/s exceeds max \
                     {:.3} m/s",
                    mob.max_speed_ms
                );
            }
        }
    }
}

/// Roosting birds must not receive boids steering (a sleeping bird does not
/// align/cohere). Force night (midnight, light ≈ 0.1 < 0.3 threshold) so the
/// tree puts birds into Roosting, then assert that on every frame an agent is
/// Roosting its velocity is exactly zero — boids must not leak into a sleeper.
#[test]
fn roosting_birds_receive_no_boids_steering() {
    let mut sim = flock_sim(2, &[0.0, 2.0], 99);
    sim.environment.time_of_day_hours = 0.0; // midnight → night roost

    let mut roost_frames = 0u64;
    for _ in 0..300 {
        sim.step(run_systems);
        for (_, (fsm, vel)) in sim.world.query::<(&FSMState, &Velocity)>().iter() {
            if *fsm == FSMState::Roosting {
                roost_frames += 1;
                assert_eq!(
                    vel.0,
                    Vector2::zeros(),
                    "Roosting bird has non-zero velocity {:?} — boids steering \
                     must be suppressed for Roosting",
                    vel.0
                );
            }
        }
    }
    assert!(
        roost_frames > 0,
        "night scenario never produced a Roosting frame — test setup broken"
    );
}
