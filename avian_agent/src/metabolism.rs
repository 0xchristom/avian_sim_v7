use hecs::World;
use avian_core::time::SimulationTime;
use avian_core::components::*;

pub fn metabolism_system(world: &mut World, time: &SimulationTime) {
    let dt = time.dt;
    
    for (_id, (meta, vel, mass)) in world.query::<(&mut Metabolism, &Velocity, &Mass)>().iter() {
        let mass_kg = mass.current_g / 1000.0;
        let v_mag = vel.0.norm();

        let bmr_kj_s = meta.bmr_watts / 1000.0;
        let cot_kj_s = 0.0125 * mass_kg * v_mag;

        meta.energy_kj -= (bmr_kj_s + cot_kj_s) * dt;
        meta.energy_kj = meta.energy_kj.max(0.0);

        // Trawienie
        if meta.crop_count > 0 && meta.gizzard_count < 10 {
            let transfer_rate = 0.1 * dt;
            let to_transfer = (transfer_rate as u32).min(meta.crop_count);
            meta.crop_count -= to_transfer;
            meta.gizzard_count += to_transfer;
            
            let energy_from_food = to_transfer as f64 * 0.5;
            let tef_loss = 0.1 * energy_from_food;
            meta.energy_kj += energy_from_food - tef_loss;
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
}