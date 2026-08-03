//! 2.6 acceptance: agents with low feather condition enter Preening and
//! restore their feathers; over a long run the FSM histogram shows a
//! preening share near the ~10% pigeon time budget.

use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use avian_core::components::{Age, FSMState, FeatherCondition};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

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
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut saw_preening = false;
    for _ in 0..120 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
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
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut agent_frames = 0u64;
    let mut preen_frames = 0u64;
    for _ in 0..5000 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
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
