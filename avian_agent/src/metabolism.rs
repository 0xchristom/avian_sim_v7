use avian_core::calibration;
use avian_core::components::*;
use avian_core::time::SimulationTime;
use hecs::World;

pub fn metabolism_system(
    world: &mut World,
    time: &SimulationTime,
    light_level: f64,
    heat_mult: f64,
    wind_flight_mult: f64,
) -> (f64, f64) {
    let dt = time.dt;
    let mut total_drained = 0.0;
    let mut total_digested = 0.0;

    // 2.3: at night (light < threshold) energy drain is reduced to the
    // calibrated fraction — resting birds metabolize slower.
    let drain_factor = if light_level < calibration::NIGHT_REST_LIGHT_THRESHOLD {
        calibration::NIGHT_DRAIN_FACTOR
    } else {
        1.0
    };

    for (_id, (meta, vel, mass, fsm)) in world
        .query::<(&mut Metabolism, &Velocity, &Mass, &FSMState)>()
        .iter()
    {
        let mass_kg = mass.current_g / 1000.0;
        let v_mag = vel.0.norm();

        // 4.1 flight: while airborne the metabolic drain scales by
        // FLIGHT_MR_MULTIPLIER (≈7× BMR). The inline mirror in `run_systems`
        // applies the same helper so the energy-balance accounting stays exact.
        // 4.4: heat scales BMR; wind scales the flight MR when airborne.
        // Phase 9: a Gliding bird's MR collapses to GLIDE_MR_MULTIPLIER
        // (near-zero) and its cost-of-transport term is ZERO — the updraft
        // supplies both lift and forward motion, so soaring is near-costless.
        let flying = v_mag >= calibration::FLIGHT_SPEED_THRESHOLD_MS;
        let wind_on_flight = if flying { wind_flight_mult } else { 1.0 };
        let gliding = *fsm == FSMState::Gliding;
        let bmr_kj_s = meta.bmr_watts
            * calibration::flight_mr_multiplier_state(v_mag, gliding)
            * heat_mult
            * wind_on_flight
            / 1000.0;
        let cot_kj_s = if gliding {
            0.0
        } else {
            0.0125 * mass_kg * v_mag
        };

        let drain = (bmr_kj_s + cot_kj_s) * dt * drain_factor;
        let actual = drain.min(meta.energy_kj);
        meta.energy_kj -= actual;
        total_drained += actual;

        // Trawienie
        if meta.crop_count > 0 && meta.gizzard_count < 10 {
            let transfer_rate = 0.1 * dt;
            let to_transfer = (transfer_rate as u32).min(meta.crop_count);
            meta.crop_count -= to_transfer;
            meta.gizzard_count += to_transfer;

            let energy_from_food = to_transfer as f64 * 0.5;
            let tef_loss = 0.1 * energy_from_food;
            meta.energy_kj += energy_from_food - tef_loss;
            total_digested += energy_from_food - tef_loss;
        }

        let blood_glucose = meta.gizzard_count as f64 * 0.5;
        let crop_ratio = if meta.crop_max > 0 {
            meta.crop_count as f64 / meta.crop_max as f64
        } else {
            0.0
        };

        meta.hunger = 0.6 * (1.0 - crop_ratio).clamp(0.0, 1.0)
            + 0.4 * (1.0 - blood_glucose / 5.0).clamp(0.0, 1.0);
        meta.hunger = meta.hunger.clamp(0.0, 1.0);

        let half_hour_bmr_kj = 0.5 * meta.bmr_watts * 3600.0 / 1000.0;
        if meta.energy_kj < half_hour_bmr_kj {
            meta.hunger = meta.hunger.max(0.8);
        }
    }

    (total_drained, total_digested)
}
