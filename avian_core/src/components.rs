use nalgebra::Vector2;
use rapier2d::prelude::RigidBodyHandle;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Position(pub Vector2<f64>);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Velocity(pub Vector2<f64>);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Heading(pub f64);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Mass {
    pub base_g: f64,
    pub condition_factor: f64,
    pub current_g: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Age {
    pub years: f64,
    pub months: u8,
    pub vitality: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Metabolism {
    pub bmr_watts: f64,
    pub energy_kj: f64,
    pub hunger: f64,
    pub crop_count: u32,
    pub gizzard_count: u32,
    pub crop_max: u32,
    pub last_peck_time: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum FSMState {
    Idle,
    Foraging,
    Fleeing,
    Scanning,
    Spacer,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LevyState {
    pub remaining_dist: f64,
    pub target_heading: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Mobility {
    pub max_speed_ms: f64,
    pub max_angular_speed_rads: f64,
    pub acceleration_ms2: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Vision {
    pub fov_degrees: f64,
    pub fovea_resolution: f64,
    pub blind_front_degrees: f64,
    pub blind_rear_degrees: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum HeadBobPhase {
    Hold,
    Thrust,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct HeadBob {
    pub phase: HeadBobPhase,
    pub offset: Vector2<f64>,
    pub time_in_phase: f64,
    pub hold_duration: f64,
    pub thrust_duration: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct PhysicsHandle(pub RigidBodyHandle);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Grain {
    pub amount: u32,
}