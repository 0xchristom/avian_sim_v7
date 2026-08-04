//! Audit 5a item 2: scripted (timer-based) bird population growth for the
//! interactive/demo run.
//!
//! This is explicitly NOT a breeding/reproduction mechanic — no birth events,
//! no parent-child relationships, no interaction with the 2.4 death/immigration
//! logic. It is a fixed, hardcoded list of `(sim_time_seconds, count_to_add)`
//! checkpoints that the server checks once per tick and satisfies by spawning
//! through the existing `spawn_agent` path, exactly like the initial
//! population does.
//!
//! Checkpoints are keyed on SIMULATION time (`frame * dt`), never wall-clock,
//! so the schedule is correct regardless of the speed multiplier (1x/10x/100x).

use avian_core::Simulation;

use crate::gerontology::spawn_agent;

/// Starting population for a scripted-population run (overrides the config's
/// `initial_agents`, which is ignored while the schedule is active).
pub const SCRIPTED_START_AGENTS: usize = 4;

/// Confirmed schedule (2026-08-04): totals of 6 at 2 min, 10 at 5 min,
/// 15 at 10 min, 20 at 15 min. The original "growing to 20" ceiling is reached
/// via a final +5 at 15 sim-min — the earlier "2/10/15 birds" reading was
/// numerically inconsistent, so it was resolved as increments reaching those
/// totals. `(sim_time_s, count_to_add)`.
pub const SCRIPTED_GROWTH_SCHEDULE: &[(f64, usize)] = &[
    (2.0 * 60.0, 2),  //  2 min -> total 6
    (5.0 * 60.0, 4),  //  5 min -> total 10
    (10.0 * 60.0, 5), // 10 min -> total 15
    (15.0 * 60.0, 5), // 15 min -> total 20
];

/// Spawn one agent at a random obstacle-free point, mirroring how the server
/// spawns the initial population (same `spawn_agent` path, same fallback).
fn spawn_scripted_agent(sim: &mut Simulation) {
    let pos = Simulation::random_free_point(
        sim.config.world_width,
        sim.config.world_height,
        &sim.obstacles,
        &mut sim.rng,
    )
    .unwrap_or_else(|| {
        nalgebra::Vector2::new(sim.config.world_width / 2.0, sim.config.world_height / 2.0)
    });
    let uid = sim.next_uid_str();
    spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
}

/// Ticker for the scripted growth schedule. Checks once per simulation tick
/// whether any checkpoint's sim-time has been reached and spawns the due
/// agents. Idempotent per checkpoint: `next` advances only after a checkpoint
/// fires, so a tick that reaches several checkpoints at once spawns them all
/// in order.
#[derive(Default)]
pub struct ScriptedGrowth {
    next: usize,
}

impl ScriptedGrowth {
    /// Spawn the starting population (`SCRIPTED_START_AGENTS`).
    pub fn spawn_start(&mut self, sim: &mut Simulation) {
        for _ in 0..SCRIPTED_START_AGENTS {
            spawn_scripted_agent(sim);
        }
    }

    /// Check the schedule at the current sim-time and spawn any agents due.
    /// Returns how many were spawned this tick (0 unless a checkpoint fired).
    pub fn tick(&mut self, sim: &mut Simulation) -> usize {
        let sim_time_s = sim.time.frame as f64 * sim.config.dt;
        let mut spawned = 0;
        while self.next < SCRIPTED_GROWTH_SCHEDULE.len()
            && sim_time_s >= SCRIPTED_GROWTH_SCHEDULE[self.next].0
        {
            let (_, count) = SCRIPTED_GROWTH_SCHEDULE[self.next];
            for _ in 0..count {
                spawn_scripted_agent(sim);
            }
            spawned += count;
            self.next += 1;
        }
        spawned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian_core::{Simulation, SimulationConfig};

    /// The schedule fires at exact sim-times regardless of `dt` (it is keyed
    /// on frame × dt), and spawns through the real `spawn_agent` path so the
    /// resulting agents are full agents (position + metabolism).
    #[test]
    fn schedule_reaches_targets_at_expected_sim_times() {
        // Fast sim-seconds so the test doesn't need 15*60*120 frames: dt = 1 s
        // means one frame = one sim-second. Deterministic per seed.
        let mut sim = Simulation::new(
            42,
            SimulationConfig {
                dt: 1.0,
                ..SimulationConfig::default()
            },
        );
        let mut growth = ScriptedGrowth::default();
        growth.spawn_start(&mut sim);
        let agent_count = |sim: &Simulation| {
            sim.world
                .query::<&avian_core::components::Metabolism>()
                .iter()
                .count()
        };
        assert_eq!(agent_count(&sim), SCRIPTED_START_AGENTS, "starts at 4");

        for frame in 0..=(15 * 60) as u64 {
            sim.time.frame = frame as u32;
            let spawned = growth.tick(&mut sim);
            match frame {
                120 => {
                    assert_eq!(spawned, 2, "fires +2 at 2 min");
                    assert_eq!(agent_count(&sim), 6, "total 6 at 2 min");
                }
                300 => {
                    assert_eq!(spawned, 4, "fires +4 at 5 min");
                    assert_eq!(agent_count(&sim), 10, "total 10 at 5 min");
                }
                600 => {
                    assert_eq!(spawned, 5, "fires +5 at 10 min");
                    assert_eq!(agent_count(&sim), 15, "total 15 at 10 min");
                }
                900 => {
                    assert_eq!(spawned, 5, "fires +5 at 15 min");
                    assert_eq!(agent_count(&sim), 20, "total 20 at 15 min");
                }
                _ => {
                    // Between checkpoints nothing extra spawns.
                    assert_eq!(spawned, 0);
                }
            }
        }
        assert_eq!(
            agent_count(&sim),
            20,
            "population holds at 20 after the last checkpoint"
        );
    }
}
