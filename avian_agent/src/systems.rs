use avian_core::Simulation;
use avian_core::components::{Position, Heading, Velocity, Metabolism, FSMState, LevyState};
use crate::search::{next_step, SearchMode};

pub fn run_systems(sim: &mut Simulation) {
    let dt = sim.config.dt;
    
    // 1. Clear and Update Spatial Grid
    sim.spatial_grid.clear();
    for (id, pos) in sim.world.query::<&Position>().iter() {
        sim.spatial_grid.insert(id, pos.0);
    }

    // 2. Metabolism & Movement
    for (_id, (pos, head, vel, meta, levy, fsm)) in sim.world.query::<(&mut Position, &mut Heading, &mut Velocity, &mut Metabolism, &mut LevyState, &mut FSMState)>().iter() {
        
        let mass_kg = 0.315; // Uproszczona masa dla testu
        let v_mag = vel.0.norm();
        let bmr_kj_s = meta.bmr_watts / 1000.0;
        let cot_kj_s = 12.5 * mass_kg * v_mag / 1000.0;
        meta.energy_kj -= (bmr_kj_s + cot_kj_s) * dt;
        
        let blood_glucose = meta.gizzard_count as f64 * 0.5;
        meta.hunger = 0.6 * (1.0 - meta.crop_count as f64 / meta.crop_max as f64)
                    + 0.4 * (1.0 - blood_glucose / 5.0).max(0.0);
                    
        if meta.energy_kj < 5.0 {
            fsm.0 = "IDLE".to_string();
            vel.0 = nalgebra::Vector2::zeros();
        } else {
            fsm.0 = "SPACER".to_string();
            let speed = 1.0;
            
            if levy.remaining_dist <= 0.0 {
                // Wymuszono jawne typowanie (f64, f64), aby rozwiązać błąd E0282
                let (dist, new_head): (f64, f64) = next_step(SearchMode::Levy, head.0, &mut sim.rng);
                levy.target_heading = new_head;
                levy.remaining_dist = dist.min(5.0);
            } else {
                levy.remaining_dist -= speed * dt;
            }
            
            let mut diff = levy.target_heading - head.0;
            while diff > std::f64::consts::PI { diff -= std::f64::consts::TAU; }
            while diff < -std::f64::consts::PI { diff += std::f64::consts::TAU; }
            let turn_rate = 2.0 * dt;
            head.0 += diff.clamp(-turn_rate, turn_rate);
            
            vel.0 = nalgebra::Vector2::new(speed * head.0.cos(), speed * head.0.sin());
        }
        
        pos.0 += vel.0 * dt;
        
        if pos.0.x < 0.5 { pos.0.x = 0.5; head.0 = std::f64::consts::PI - head.0; levy.target_heading = head.0; }
        if pos.0.x > 31.5 { pos.0.x = 31.5; head.0 = std::f64::consts::PI - head.0; levy.target_heading = head.0; }
        if pos.0.y < 0.5 { pos.0.y = 0.5; head.0 = -head.0; levy.target_heading = head.0; }
        if pos.0.y > 20.5 { pos.0.y = 20.5; head.0 = -head.0; levy.target_heading = head.0; }
    }
}