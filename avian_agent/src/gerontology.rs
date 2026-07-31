use hecs::World;
use rand_distr::Distribution; // Dodany brakujący import
use avian_core::rng::SimRng;
use avian_core::components::*;
use crate::components::*;

pub fn sample_age(rng: &mut SimRng) -> Age {
    let u: f64 = rng.gen();
    let target_s: f64 = 1.0 - u; // Przeżywalność S(x)
    
    // Numeryczna inwersja CDF dla Gompertz-Makeham: h(x) = 0.001 * exp(0.3*x) + 0.01
    let mut low: f64 = 0.5;
    let mut high: f64 = 15.0;
    
    for _ in 0..30 {
        let mid: f64 = (low + high) / 2.0;
        // S(x) = exp(- (A/B)*(exp(B*x) - 1) - C*x)
        let s_mid: f64 = (-((0.001 / 0.3) * ((0.3 * mid).exp() - 1.0) - 0.01 * mid)).exp();
        if s_mid > target_s {
            low = mid;
        } else {
            high = mid;
        }
    }
    
    let years: f64 = (low + high) / 2.0;
    let total_months = (years * 12.0).round() as u32;
    Age {
        years,
        months: (total_months % 12) as u8,
        vitality: 1.0,
    }
}

pub fn mass_from_age(age: &Age, rng: &mut SimRng) -> Mass {
    let base_mass = if age.years < 1.0 {
        250.0 + (age.years - 0.5) * 130.0
    } else if age.years <= 8.0 {
        315.0
    } else {
        315.0 - (age.years - 8.0) * 5.0
    };
    
    let condition = (rand_distr::Normal::new(0.0f64, 0.025f64).unwrap().sample(rng)).clamp(-0.05, 0.05);
    let current_g = base_mass * (1.0 + condition);
    
    Mass {
        base_g: base_mass,
        condition_factor: condition,
        current_g,
    }
}

pub fn vitality_update(age: &mut Age, dt: f64, rng: &mut SimRng) {
    let decay = 0.001 * (0.3 * age.years).exp() + 0.01;
    let noise = rand_distr::Normal::new(0.0f64, 0.01f64).unwrap().sample(rng);
    age.vitality += -decay * dt + noise * dt.sqrt();
    if age.vitality < 0.0 {
        age.vitality = 0.0;
    }
}

pub fn spawn_agent(world: &mut World, rng: &mut SimRng, pos: nalgebra::Vector2<f64>) -> hecs::Entity {
    let age = sample_age(rng);
    let mass = mass_from_age(&age, rng);
    let mass_kg = mass.current_g / 1000.0;
    
    world.spawn((
        Position(pos),
        Velocity(nalgebra::Vector2::zeros()),
        Heading(rng.gen_range(0.0..std::f64::consts::TAU)),
        mass,
        age,
        Metabolism {
            bmr_watts: 10.5,
            energy_kj: 50.0,
            hunger: 0.5,
            crop_count: 0,
            gizzard_count: 0,
            crop_max: (mass.current_g / 10.0).ceil() as u32,
            last_peck_time: 0.0,
        },
        Mobility {
            max_speed_ms: 1.2,
            max_angular_speed_rads: 2.0,
            acceleration_ms2: 10.0 * mass_kg.powf(-0.25),
        },
        Vision {
            fov_degrees: 170.0,
            fovea_resolution: 1.0,
            blind_front_degrees: 20.0,
            blind_rear_degrees: 30.0,
        },
    ))
}