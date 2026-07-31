use nalgebra::Vector2;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FSMState(pub String);

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LevyState {
    pub remaining_dist: f64,
    pub target_heading: f64,
}