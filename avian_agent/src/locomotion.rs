use nalgebra::Vector2;
use avian_core::components::*;
use avian_core::components::HeadBobPhase;

pub struct VaultingGait {
    pub leg_length_m: f64,
    pub duty_factor: f64,
    pub stance_phase: f64,
}

impl VaultingGait {
    pub fn com_height(&self, phase: f64) -> f64 {
        let t = phase / self.duty_factor;
        self.leg_length_m * (1.0 - 0.2 * t * (1.0 - t))
    }
    
    pub fn update(&mut self, vel: &Velocity, dt: f64) -> f64 {
        let freq = 2.0 + vel.0.norm() * 1.5;
        self.stance_phase += freq * dt;
        if self.stance_phase > 1.0 {
            self.stance_phase -= 1.0;
        }
        self.com_height(self.stance_phase)
    }
}

pub struct HeadBobSystem {
    pub time_in_phase: f64,
    pub hold_duration: f64,
    pub thrust_duration: f64,
    pub current_phase: HeadBobPhase,
}

impl HeadBobSystem {
    pub fn update(&mut self, vel: &Velocity, heading: f64, dt: f64) -> (HeadBobPhase, Vector2<f64>) {
        let v_mag = vel.0.norm();
        let optic_flow = v_mag;
        
        self.time_in_phase += dt;
        
        let mut offset = Vector2::zeros();
        
        if optic_flow < 3.0 {
            self.current_phase = HeadBobPhase::Hold;
            self.time_in_phase = 0.0;
        } else {
            if matches!(self.current_phase, HeadBobPhase::Hold) && self.time_in_phase >= self.hold_duration {
                self.current_phase = HeadBobPhase::Thrust;
                self.time_in_phase = 0.0;
            } else if matches!(self.current_phase, HeadBobPhase::Thrust) && self.time_in_phase >= self.thrust_duration {
                self.current_phase = HeadBobPhase::Hold;
                self.time_in_phase = 0.0;
            }
            
            if matches!(self.current_phase, HeadBobPhase::Thrust) {
                let t = self.time_in_phase / self.thrust_duration;
                let jerk = 10.0 * t.powi(3) - 15.0 * t.powi(4) + 6.0 * t.powi(5);
                let distance = 0.15;
                offset.x = distance * jerk * heading.cos();
                offset.y = distance * jerk * heading.sin();
            }
        }
        
        (self.current_phase, offset)
    }
}