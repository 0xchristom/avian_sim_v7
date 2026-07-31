use hecs::World;
use rand_distr::Distribution;
use avian_core::rng::SimRng;
use avian_core::components::*;
use crate::components::*;

/// Prawidłowa formuła Gompertz-Makeham:
/// h(x) = A * exp(B*x) + C
/// S(x) = exp( -(A/B)*(exp(B*x) - 1) - C*x )
pub fn sample_age(rng: &mut SimRng) -> Age {
    let u: f64 = rng.gen();
    let target_s: f64 = u;

    let mut low: f64 = 0.5;
    let mut high: f64 = 15.0;

    for _ in 0..40 {
        let mid: f64 = (low + high) / 2.0;
        let a = 0.001;
        let b = 0.3;
        let c = 0.01;
        
        // POPRAWKA: minus przed C*x
        let s_mid: f64 = (-(a / b) * ((b * mid).exp() - 1.0) - c * mid).exp();
        
        if s_mid > target_s {
            low = mid;
        } else {
            high = mid;
        }
    }

    let years: f64 = (low + high) / 2.0;
    let total_months = (years * 12.0).round() as u32;
    
    let initial_vitality: f64 = rng.gen_range(0.85..1.0);
    
    Age {
        years,
        months: (total_months % 12) as u8,
        vitality: initial_vitality,
    }
}

pub fn mass_from_age(age: &Age, rng: &mut SimRng) -> Mass {
    let base_mass = if age.years < 1.0 {
        250.0 + (age.years - 0.5).max(0.0) / 0.5 * 65.0
    } else if age.years <= 8.0 {
        315.0
    } else {
        315.0 - (age.years - 8.0) * 5.0
    }.max(200.0);

    let condition = rand_distr::Normal::new(0.0f64, 0.025f64)
        .unwrap()
        .sample(rng)
        .clamp(-0.05, 0.05);
    
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

    // Wariancja inicjalizacji
    let base_energy = 40.0 + rng.gen_range(0.0..20.0);
    let initial_crop = rng.gen_range(0..=3);
    let initial_gizzard = rng.gen_range(0..=2);

    world.spawn((
        Position(pos),
        Velocity(nalgebra::Vector2::zeros()),
        Heading(rng.gen_range(0.0..std::f64::consts::TAU)),
        mass,
        age,
        Metabolism {
            bmr_watts: 10.5,
            energy_kj: base_energy,
            hunger: 0.0,
            crop_count: initial_crop,
            gizzard_count: initial_gizzard,
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