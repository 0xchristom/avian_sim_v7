use avian_core::{Simulation, SimulationConfig};
use avian_agent::systems::run_systems;
use avian_agent::gerontology::spawn_agent;
use bincode;

fn setup_sim() -> Simulation {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    for i in 0..20 {
        let pos = nalgebra::Vector2::new(i as f64 * 1.5, 10.0);
        spawn_agent(&mut sim.world, &mut sim.rng, pos);
    }
    sim
}

#[test]
fn test_bit_perfect_reproducibility_with_systems() {
    let mut sim1 = setup_sim();
    let mut sim2 = setup_sim();

    for _ in 0..100 {
        sim1.step(|s, dt| run_systems(s, dt));
        sim2.step(|s, dt| run_systems(s, dt));
    }

    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();

    assert_eq!(sim1.time.frame, 100, "Frame counter should be exactly 100");
    assert_eq!(snap1, snap2, "Snapshots diverge! Determinism broken.");
}