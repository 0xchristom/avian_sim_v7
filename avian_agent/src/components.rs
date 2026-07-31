use hecs::Entity;
use nalgebra::Vector2;
use serde::{Serialize, Deserialize};

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

// Usunięto Serialize/Deserialize, ponieważ hecs::Entity tego nie implementuje
#[derive(Clone, Copy, Debug)]
pub struct HeadBob {
    pub phase: HeadBobPhase,
    pub head_offset: Vector2<f64>,
    pub head_body: Entity,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum HeadBobPhase {
    Hold,
    Thrust,
}