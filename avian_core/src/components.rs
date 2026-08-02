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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum FSMState {
    Idle,
    Foraging,
    Fleeing,
    Scanning,
    Spacer,
    /// 2.6 — feathers_condition below threshold; restores feathers in place.
    Preening,
    /// 2.7 — vitality below SICK_VITALITY_THRESHOLD; moves 50% slower and is
    /// more vulnerable to predators.
    Sick,
}

/// 2.6 feather condition 0..=1 (1 = pristine). Decays with rain/mud, restored
/// by preening. Trigger for `FSMState::Preening`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FeatherCondition(pub f64);

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PhysicsHandle(pub u64); // Neutralny typ (Ticket R2-12)

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Grain {
    pub amount: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Weather {
    Clear,
    Rain,
    Wind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EnvironmentState {
    pub time_of_day_hours: f64,
    pub light_level: f64,
    pub weather: Weather,
}

impl Default for EnvironmentState {
    fn default() -> Self {
        // 2.3 dependency note: light_level defaults to day (1.0) so the 2.0
        // NightRest condition (light < 0.3) is naturally inactive until 2.3
        // ships the day/night cycle — the gap is now one sprint, not seven.
        Self {
            time_of_day_hours: 12.0,
            light_level: 1.0,
            weather: Weather::Clear,
        }
    }
}

/// Stable per-run agent/predator identity (3.3). Format: `A{session:04}-{id:06}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentUid(pub String);

/// 2.2 predator entity — a bespoke pursuit script, NOT a BTNode.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Predator {
    pub speed_multiplier: f64,
    pub detection_radius: f64,
    pub capture_cooldown: u32,
    /// Patrol waypoint used when no agent is inside the detection radius —
    /// keeps the predator ranging across the map instead of pinning one spot.
    pub patrol_target: Option<Vector2<f64>>,
    /// Seconds until this predator despawns (2.2b). Drawn once from
    /// `[PREDATOR_LIFETIME_MIN_S, PREDATOR_LIFETIME_MAX_S]` at spawn; when it
    /// reaches 0 the entity is removed and `RemovePredator` is logged.
    pub lifetime_remaining_s: f64,
}

/// Per-frame threat flag for an agent (2.2 telemetry + obs input).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Alarm(pub bool);

/// Previous frame's `Alarm` — lets 3.2 detect a fleeing episode that ended
/// safely (alarm fired last frame, cleared this frame, no capture) for the
/// +0.5 flee-success reward.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlarmPrev(pub bool);