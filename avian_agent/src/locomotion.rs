use avian_core::components::HeadBobPhase;
use avian_core::components::*;
use nalgebra::Vector2;

pub struct VaultingGait {
    pub leg_length_m: f64,
    pub duty_factor: f64,
    pub stance_phase: f64,
}

impl VaultingGait {
    pub fn com_height(&self, phase: f64) -> f64 {
        // Fix #11: Clamp phase to duty_factor so t stays in [0, 1].
        // Previously t could exceed 1.0 → t*(1-t) negative → COM height > leg length.
        let t = (phase / self.duty_factor).clamp(0.0, 1.0);
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
    pub fn update(
        &mut self,
        vel: &Velocity,
        heading: f64,
        dt: f64,
    ) -> (HeadBobPhase, Vector2<f64>) {
        let v_mag = vel.0.norm();
        let optic_flow = v_mag;

        self.time_in_phase += dt;

        let mut offset = Vector2::zeros();

        // Fix: threshold was 3.0 but max speed is 1.2 m/s — head bob never activated.
        // Real pigeons head-bob at walking speeds (~0.3 m/s and above).
        if optic_flow < 0.3 {
            self.current_phase = HeadBobPhase::Hold;
            self.time_in_phase = 0.0;
        } else {
            if matches!(self.current_phase, HeadBobPhase::Hold)
                && self.time_in_phase >= self.hold_duration
            {
                self.current_phase = HeadBobPhase::Thrust;
                self.time_in_phase = 0.0;
            } else if matches!(self.current_phase, HeadBobPhase::Thrust)
                && self.time_in_phase >= self.thrust_duration
            {
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
