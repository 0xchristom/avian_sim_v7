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
    /// Phase 9 (Audit 3): soaring in a building updraft. MR collapses to
    /// `GLIDE_MR_MULTIPLIER` and steering agility is restricted (bird rides the
    /// thermal in a straight-ish line instead of maneuvering).
    Gliding,
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
    /// Appended last so pre-4.4 bincode checkpoints keep their discriminants.
    Heat,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct EnvironmentState {
    pub time_of_day_hours: f64,
    pub light_level: f64,
    pub weather: Weather,
    /// 4.4: 0..1 smooth blend toward the active weather (1.0 when weather is
    /// non-Clear, 0.0 in Clear). Effects scale by this, so weather ramps
    /// in/out over ~1 sim-second instead of snapping.
    pub weather_intensity: f64,
    /// 4.4: frames until the weather scheduler re-rolls the global state.
    pub weather_frames_left: u32,
    /// 4.4: global wind direction (radians); re-rolled when Wind starts.
    pub wind_heading: f64,
    /// Phase 9 (Audit 3): horizontal direction the sun is coming from (radians),
    /// derived from `time_of_day_hours`. Drives which side of each Building the
    /// thermal updraft forms on (sun-facing face). Sunrise (~6h) = east (0),
    /// noon (12h) = south (-π/2), sunset (18h) = west (π). 0 = +x, π/2 = +y.
    pub sun_heading: f64,
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
            weather_intensity: 0.0,
            weather_frames_left: crate::calibration::WEATHER_UPDATE_INTERVAL_FRAMES,
            wind_heading: 0.0,
            sun_heading: -std::f64::consts::FRAC_PI_2, // noon default: sun from the SOUTH (matches -(h-6)/12·π derivation)
        }
    }
}

/// Stable per-run agent/predator identity (3.3). Format: `A{session:04}-{id:06}`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentUid(pub String);

/// 6.2: predator hunt-state machine — the dynamic speed scale 1-5 (await slow,
/// chase ramps to very fast) with a 1-second busy beat after a strike/miss.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PredatorHuntState {
    /// No prey inside the detection radius — slow patrol (speed level 1).
    Await,
    /// Prey detected — pursues at a speed level that ramps toward 5.
    Chase,
    /// Just struck (capture or miss) — halted "busy" for `PREDATOR_CATCH_BUSY_S`.
    Catch,
}

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
    /// 6.2: captures eaten so far; at `predator_fill_meals_target` (when
    /// `predator_fill_meals` is enabled) the predator despawns satisfied.
    pub meals_eaten: u32,
    /// 6.2: current hunt-state machine state.
    pub hunt_state: PredatorHuntState,
    /// 6.2: current dynamic speed tier on the 1 (slow)..5 (very fast) scale.
    pub speed_level: u8,
    /// 6.2: seconds left in the `Catch` busy beat (0 when not catching).
    pub hunt_timer_s: f64,
}

/// 4.2 spatial memory: remembered food locations with decaying strength.
/// Feeds the existing Forage condition as a target-picker (NOT a new condition
/// node) — when no grain is visible, `foraging_action` picks a slot weighted by
/// memory strength, else falls through to `Wander`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemorySlots {
    pub slots: Vec<MemorySlot>,
}

/// One remembered food location (4.2). Strength 1.0 on write, decays over
/// `MEMORY_DECAY_FRAMES`; LRU-evicted at `MEMORY_SLOTS_MAX`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MemorySlot {
    pub pos: Vector2<f64>,
    pub strength: f64,
    /// Frames until this memory fades (600-frame decay, 4.2).
    pub ttl_frames: u32,
}

impl Default for MemorySlots {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

/// Per-frame threat flag for an agent (2.2 telemetry + obs input).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Alarm(pub bool);

/// Previous frame's `Alarm` — lets 3.2 detect a fleeing episode that ended
/// safely (alarm fired last frame, cleared this frame, no capture) for the
/// +0.5 flee-success reward.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct AlarmPrev(pub bool);

/// 4.3: obstacle classification for the urban map. Visual distinction only —
/// all obstacle kinds share the same static-box physics (block movement) and
/// the same ray-cast line-of-sight occlusion (block vision).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ObstacleKind {
    Building,
    Wall,
    Water,
    Tree,
}

/// 4.3: a static box obstacle spanning `min..=max` in world meters. NOT a
/// world entity — obstacles are plain data on `Simulation` (they never move),
/// with a matching fixed collider registered in `PhysicsWorld`. Kept out of
/// the hecs world so checkpoints don't need a component column for them.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Obstacle {
    pub id: u32,
    pub kind: ObstacleKind,
    pub min: Vector2<f64>,
    pub max: Vector2<f64>,
}

/// Phase 9 (Audit 3): an invisible updraft zone on the sun-facing side of a
/// `ObstacleKind::Building`. A bird that is airborne, inside `min..=max`, and
/// whose heading aligns with `flow` switches to `FSMState::Gliding`.
/// Re-derived every tick from `(obstacles, sun_heading)` — never serialized,
/// never stored, deterministic.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ThermalZone {
    pub min: Vector2<f64>,
    pub max: Vector2<f64>,
    /// Direction of the updraft/airflow (unit vector, axis-aligned). The bird's
    /// heading must be within `GLIDE_HEADING_ALIGN_DEG` of this to glide.
    pub flow: Vector2<f64>,
}