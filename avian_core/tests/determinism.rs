use avian_core::{Simulation, SimulationConfig};

#[test]
fn test_bit_perfect_reproducibility() {
    let mut sim1 = Simulation::new(42, SimulationConfig::default());
    let mut sim2 = Simulation::new(42, SimulationConfig::default());
    
    for _ in 0..1000 {
        sim1.step();
        sim2.step();
    }
    
    let snap1 = bincode::serialize(&sim1.snapshot()).unwrap();
    let snap2 = bincode::serialize(&sim2.snapshot()).unwrap();
    
    assert_eq!(snap1, snap2);
}
