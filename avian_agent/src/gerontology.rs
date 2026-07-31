use hecs::World;
use rand_distr::Distribution;
use avian_core::rng::SimRng;
use avian_core::components::{Position, Velocity, Heading, Mass, Age, Metabolism, FSMState, LevyState};

pub fn sample_age(rng: &mut SimRng) -> Age {
    let u: f64 = rng.gen();
    let target_s: f64 = 1.0 - u;
    
    let mut low: f64 = 0.5;
    let mut high: f64 = 15.0;
    
    for _ in 0..30 {
        let mid: f64 = (low + high) / 2.0;
        let gompertz = (0.001 / 0.3) * ((0.3 * mid).exp() - 1.0);
        let makeham = 0.01 * mid;
        let s_mid = (-(gompertz + makeham)).exp();
        
        if s_mid > target_s { low = mid; } else { high = mid; }
    }
    
    let years: f64 = (low + high) / 2.0;
    let total_months = (years * 12.0).round() as u32;
    Age { years, months: (total_months % 12) as u8, vitality: 1.0 }
}

pub fn mass_from_age(age: &Age, rng: &mut SimRng) -> Mass {
    let base_mass = if age.years < 1.0 { 250.0 + (age.years - 0.5) * 130.0 }
    else if age.years <= 8.0 { 315.0 }
    else { 315.0 - (age.years - 8.0) * 5.0 };
    
    let condition = (rand_distr::Normal::new(0.0f64, 0.025f64).unwrap().sample(rng)).clamp(-0.05, 0.05);
    Mass { base_g: base_mass, condition_factor: condition, current_g: base_mass * (1.0 + condition) }
}

pub fn spawn_agent(world: &mut World, rng: &mut SimRng, pos: nalgebra::Vector2<f64>) -> hecs::Entity {
    let age = sample_age(rng);
    let mass = mass_from_age(&age, rng);
    let crop_max = (mass.current_g / 10.0).ceil() as u32;
    
    world.spawn((
        Position(pos),
        Velocity(nalgebra::Vector2::zeros()),
        Heading(rng.gen_range(0.0..std::f64::consts::TAU)),
        mass,
        age,
        Metabolism {
            bmr_watts: 10.5,
            energy_kj: 40.0 + rng.gen_range(0.0..20.0),
            hunger: 0.2,
            crop_count: crop_max / 2,
            gizzard_count: 3,
            crop_max,
            last_peck_time: 0.0,
        },
        FSMState("SPACER".to_string()),
        LevyState { remaining_dist: 0.0, target_heading: rng.gen_range(0.0..std::f64::consts::TAU) },
    ))
}