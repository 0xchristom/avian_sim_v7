use avian_core::calibration;
use avian_core::components::*;
use avian_core::rng::SimRng;
use hecs::World;
use rand_distr::Distribution;

pub fn sample_age(rng: &mut SimRng) -> Age {
    // Stable-age population structure: age density ∝ S(t), the SAME survival
    // curve the vitality model uses (4.0 Weibull, median 4 yr, max 15 yr).
    // Most new arrivals are young/healthy and only a small minority (~12%)
    // are old/sick — matching a real flock. Rejection sampling over
    // t ~ U[0, max] with accept probability S(t) ≈ 28% → ~3.5 draws.
    //
    // (Previous versions inverted the survival CDF, i.e. sampled a death
    // COHORT: median ~4 yr but ~30% born Sick, skewing the FSM time budget
    // and flattening the 2.7 anomaly signal.)
    let years = loop {
        let t = rng.gen::<f64>() * calibration::WILD_MAX_LIFESPAN_YEARS;
        let s = calibration::vitality_at(t);
        let u: f64 = rng.gen();
        if u <= s {
            break t;
        }
    };
    let total_months = (years * 12.0).round() as u32;
    Age {
        years,
        months: (total_months % 12) as u8,
        vitality: calibration::vitality_at(years),
    }
}

/// Gompertz-Makeham mortality hazard (per year): h(t) = A·e^{B·t} + C.
/// Used by 2.4 for stochastic death rolls. Params stay aligned with the
/// survival curve used in `sample_age` (A=0.002, B=0.6, C=0.02).
pub fn mortality_hazard(years: f64) -> f64 {
    0.002 * (0.6 * years).exp() + 0.02
}

pub fn mass_from_age(age: &Age, rng: &mut SimRng) -> Mass {
    // 4.1: mass curve follows calibration (15g hatchling → 200g fledgling →
    // 315g adult); juveniles interpolate from fledgling toward adult.
    let base_mass = if age.years < 1.0 {
        calibration::FLEDGLING_MASS_G
            + (calibration::ADULT_MASS_G - calibration::FLEDGLING_MASS_G)
                * (age.years / 1.0).clamp(0.0, 1.0)
    } else if age.years <= 8.0 {
        calibration::ADULT_MASS_G
    } else {
        calibration::ADULT_MASS_G - (age.years - 8.0) * 5.0
    };

    let condition = (rand_distr::Normal::new(0.0f64, 0.025f64)
        .unwrap()
        .sample(rng))
    .clamp(-0.05, 0.05);
    Mass {
        base_g: base_mass,
        condition_factor: condition,
        current_g: base_mass * (1.0 + condition),
    }
}

pub fn spawn_agent(
    world: &mut World,
    rng: &mut SimRng,
    pos: nalgebra::Vector2<f64>,
    physics: &mut avian_physics::PhysicsWorld,
    uid: String,
) -> hecs::Entity {
    let age = sample_age(rng);
    let mass = mass_from_age(&age, rng);
    let mass_kg = mass.current_g / 1000.0;
    let crop_max = (mass.current_g / 10.0).ceil() as u32;

    let rb_handle = physics.spawn_agent_body(
        nalgebra::Vector2::new(pos.x as f32, pos.y as f32),
        mass_kg as f32,
    );

    let e = world.spawn((
        Position(pos),
        Velocity(nalgebra::Vector2::zeros()),
        Heading(rng.gen_range(0.0..std::f64::consts::TAU)),
        mass,
        age,
        Metabolism {
            bmr_watts: calibration::bmr_for_mass(mass.current_g),
            energy_kj: 40.0 + rng.gen_range(0.0..20.0),
            hunger: 0.2,
            crop_count: crop_max / 2,
            gizzard_count: 3,
            crop_max,
            last_peck_time: 0.0,
        },
        FSMState::Spacer,
        LevyState {
            remaining_dist: 0.0,
            target_heading: rng.gen_range(0.0..std::f64::consts::TAU),
        },
        Mobility {
            max_speed_ms: calibration::WALK_SPEED_MS,
            max_angular_speed_rads: 2.0,
            acceleration_ms2: 10.0 * mass_kg.powf(-0.25),
        },
        Vision {
            fov_degrees: calibration::VISION_FOV_DEGREES,
            fovea_resolution: 1.0,
            blind_front_degrees: 20.0,
            blind_rear_degrees: 30.0,
        },
        HeadBob {
            phase: HeadBobPhase::Hold,
            offset: nalgebra::Vector2::zeros(),
            time_in_phase: 0.0,
            hold_duration: 0.1,
            thrust_duration: 0.05,
        },
        FeatherCondition(calibration::FEATHER_CONDITION_DEFAULT),
        PhysicsHandle(rb_handle),
        AgentUid(uid),
        // NOTE: `Alarm` + `AlarmPrev` are inserted via `world.insert` below —
        // hecs 0.10 does NOT register components spawned inside a NESTED tuple
        // bundle (queries for them return empty). A top-level 2-tuple insert
        // registers both correctly.
    ));
    world
        .insert(e, (Alarm(false), AlarmPrev(false), MemorySlots::default()))
        .expect("insert alarm components");
    e
}
