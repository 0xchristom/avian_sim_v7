#![recursion_limit = "256"]

pub mod calibration;
pub mod checkpoint;
pub mod components;
pub mod events;
pub mod rng;
pub mod spatial;
pub mod time;

use avian_physics::PhysicsWorld;
use components::*;
use events::Event;
use hecs::World;
use nalgebra::Vector2;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Sprint 1 (Audit 5): structural, readable config-validation error. Each
/// field violating a constraint is reported by name so a bad `simulation.toml`
/// or a wrong programmatic config is obvious instead of failing later in a
/// wall-clock mismatch, NaN body, or an infinite fixed-step loop.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid simulation config: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulationConfig {
    pub dt: f64,
    pub gravity: f64,
    pub max_agents: usize,
    /// 2.2b: when true, predators get a randomized 5-15 s lifetime and despawn
    /// when it elapses. Headless flee/capture benchmarks disable it to keep a
    /// persistent predator (the two behaviors have separate acceptance tests).
    pub predator_expiry: bool,
    /// 6.2: when true, a predator despawns after eating `predator_fill_meals_target`
    /// pigeons (captures) — the "disappears after 3 meals" request. Existing
    /// persistent-predator acceptance tests disable both this and
    /// `predator_expiry` to keep a predator on the map for the whole run.
    pub predator_fill_meals: bool,
    /// 6.2: captures needed before a fill-meals predator despawns.
    pub predator_fill_meals_target: u32,
    /// 4.2: when true, immigration respawns keep the population at
    /// MIN_POPULATION. Deterministic single-bird tests (e.g. memory-biased
    /// foraging) disable it so no flock is auto-spawned (boids would perturb
    /// the target bird's straight-line path).
    pub immigration_enabled: bool,
    /// 4.3: when true, `Simulation::new` builds the default urban map (a few
    /// static box buildings) on top of the empty 32×21 arena. Off by default
    /// so the existing deterministic test scenarios keep their exact
    /// trajectories. Obstacles are added by `add_obstacle` either way.
    pub urban_obstacles: bool,
    /// 4.4: when true, the stochastic weather scheduler runs (re-rolls
    /// Clear/Rain/Wind/Heat every ~5 sim-seconds with smooth 1-s ramps). Off
    /// by default so the existing deterministic scenarios keep Clear weather
    /// and their exact trajectories. Weather can still be forced per-run via
    /// the `SetWeather` event regardless of this flag.
    pub weather_enabled: bool,
    // ---- 5.2 scenario params (simulation.toml). All default to the current
    // fixed behavior so an empty file == SimulationConfig::default(). ----
    /// 5.2: run seed. `None` → the caller's explicit `Simulation::new(seed, ..)`
    /// seed wins (or `Simulation::from_config` falls back to 42).
    pub seed: Option<u64>,
    /// 5.2: arena size in meters. The four walls are built at these bounds.
    pub world_width: f64,
    pub world_height: f64,
    /// 5.2: initial population/grains spawned by the server at startup.
    pub initial_agents: usize,
    pub initial_grains: usize,
    /// 5.2: real-time pacing multiplier for the interactive server (1× = 60fps
    /// pacing). Ignored in headless mode, where frames are the unit.
    pub time_scale: f64,
    /// Audit 4 §9.7: length of one full day/night cycle in sim-seconds (drives
    /// the `light_level` sinusoid in `systems.rs`). Scenario-tunable so the
    /// interactive view can use a longer, more legible cycle than the compiled
    /// calibration default (600 s) that headless data-generation runs use.
    /// The default is the biology constant — an empty/omitted toml keeps the
    /// exact current behavior.
    pub day_length_sim_s: f64,
    /// 5.2: hunger threshold that flips the root Forage condition. Scenario-
    /// tunable; the biology constant FORAGING_HUNGER_THRESHOLD is the default.
    pub foraging_threshold: f64,
    /// 5.2: when false, boids steering is suppressed entirely (agents move by
    /// the behavior tree alone — the plan's per-run scenario experiment).
    pub flocking_enabled: bool,
    /// 5.2: explicit obstacle layout (overrides the `urban_obstacles` default
    /// map when non-empty). Scenario geometry, separate from the compile-time
    /// biology constants.
    pub obstacles: Vec<ObstacleSpec>,
    /// 5.2: pre-recorded scenario events, injected at frame 0 (same semantics
    /// as the server's `--events-file`).
    pub event_schedule: Vec<Event>,
}

/// 5.2: a scenario obstacle in toml-space. Same box semantics as the runtime
/// `Obstacle` (min..=max), but without the derived `id` — `Simulation::new`
/// assigns ids in order when it materializes the layout.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct ObstacleSpec {
    pub kind: ObstacleKind,
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            dt: 1.0 / 120.0,
            gravity: 0.0,
            max_agents: 1000,
            predator_expiry: true,
            predator_fill_meals: true,
            predator_fill_meals_target: calibration::PREDATOR_FILL_MEALS_TARGET,
            immigration_enabled: true,
            urban_obstacles: false,
            weather_enabled: false,
            seed: None,
            world_width: calibration::WORLD_WIDTH_M,
            world_height: calibration::WORLD_HEIGHT_M,
            initial_agents: 30,
            initial_grains: 15,
            time_scale: 1.0,
            day_length_sim_s: calibration::DAY_LENGTH_SIM_S,
            foraging_threshold: calibration::FORAGING_HUNGER_THRESHOLD,
            flocking_enabled: true,
            obstacles: Vec::new(),
            event_schedule: Vec::new(),
        }
    }
}

impl SimulationConfig {
    /// Sprint 1 (Audit 5): validate the config against the invariants the
    /// simulation depends on. Rejects `dt <= 0`, NaN, infinity, non-positive
    /// world dimensions, `time_scale <= 0`, non-positive `day_length_sim_s`,
    /// zero `max_agents`, absurd spawn counts, and non-finite gravity.
    ///
    /// The error is structural and readable: one human message naming the
    /// offending field and value, so a bad `simulation.toml` or a wrong
    /// programmatic config fails loudly at construction instead of later as a
    /// NaN body, a zero-step infinite loop, or a wall-clock mismatch.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.dt.is_nan() || self.dt.is_infinite() || self.dt <= 0.0 {
            return Err(ConfigError(format!(
                "dt must be finite and > 0 (got {}) — a zero/NaN/infinite step can never advance the clock",
                self.dt
            )));
        }
        if self.gravity.is_nan() || self.gravity.is_infinite() {
            return Err(ConfigError(format!(
                "gravity must be finite (got {})",
                self.gravity
            )));
        }
        if self.max_agents == 0 {
            return Err(ConfigError(format!(
                "max_agents must be >= 1 (got {})",
                self.max_agents
            )));
        }
        if !(self.world_width.is_finite() && self.world_width > 0.0) {
            return Err(ConfigError(format!(
                "world_width must be finite and > 0 (got {})",
                self.world_width
            )));
        }
        if !(self.world_height.is_finite() && self.world_height > 0.0) {
            return Err(ConfigError(format!(
                "world_height must be finite and > 0 (got {})",
                self.world_height
            )));
        }
        if self.time_scale.is_nan() || self.time_scale.is_infinite() || self.time_scale <= 0.0 {
            return Err(ConfigError(format!(
                "time_scale must be finite and > 0 (got {})",
                self.time_scale
            )));
        }
        if self.day_length_sim_s.is_nan()
            || self.day_length_sim_s.is_infinite()
            || self.day_length_sim_s <= 0.0
        {
            return Err(ConfigError(format!(
                "day_length_sim_s must be finite and > 0 (got {})",
                self.day_length_sim_s
            )));
        }
        if self.foraging_threshold.is_nan()
            || self.foraging_threshold.is_infinite()
            || self.foraging_threshold < 0.0
        {
            return Err(ConfigError(format!(
                "foraging_threshold must be finite and >= 0 (got {})",
                self.foraging_threshold
            )));
        }
        // 6.2: absurd spawn values would try to instantiate an unbuildable
        // world (or exceed the population cap the sim enforces).
        if self.initial_agents > self.max_agents {
            return Err(ConfigError(format!(
                "initial_agents ({}) exceeds max_agents ({})",
                self.initial_agents, self.max_agents
            )));
        }
        if self.initial_agents > 100_000 || self.initial_grains > 1_000_000 {
            return Err(ConfigError(format!(
                "absurd spawn counts: initial_agents={} initial_grains={}",
                self.initial_agents, self.initial_grains
            )));
        }
        // Obstacle boxes must be valid and live inside the arena.
        for (i, o) in self.obstacles.iter().enumerate() {
            let (min, max) = (o.min, o.max);
            if min[0].is_nan()
                || min[1].is_nan()
                || max[0].is_nan()
                || max[1].is_nan()
                || max[0] <= min[0]
                || max[1] <= min[1]
            {
                return Err(ConfigError(format!(
                    "obstacle[{i}] has an invalid box min={min:?} max={max:?} — needs max > min per axis"
                )));
            }
            if min[0] < 0.0
                || min[1] < 0.0
                || max[0] > self.world_width
                || max[1] > self.world_height
            {
                return Err(ConfigError(format!(
                    "obstacle[{i}] min={min:?} max={max:?} is outside the {world_width}x{world_height} arena",
                    world_width = self.world_width,
                    world_height = self.world_height,
                )));
            }
        }
        Ok(())
    }

    /// 5.2: read a `simulation.toml` file. Any field the file omits falls back
    /// to the compiled default — biology constants stay in `calibration.rs`,
    /// only scenario params belong in the file (see DEVELOPMENT_PLAN §5.2).
    /// Sprint 1: the loaded config is validated before it is returned.
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let s = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&s)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// 5.2: serialize back to toml (used to round-trip a written file).
    pub fn to_toml_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub uid: String,
    pub pos: [f64; 2],
    pub heading: f64,
    pub vel: [f64; 2],
    pub mass_g: f64,
    pub age_years: f64,
    pub energy_kj: f64,
    pub hunger: f64,
    /// Sprint 2 (Audit 5, B20): the FSM as a compact enum discriminant.
    /// Serde serializes unit-variant enums as their variant name string, so the
    /// viewer JSON is unchanged from the previous `String` field.
    pub fsm_state: components::FSMState,
    pub head_offset: [f64; 2],
    pub alarm_triggered: bool,
    /// 2.7 anomaly ground-truth label — vitality below SICK_VITALITY_THRESHOLD.
    pub sick: bool,
    /// 4.0 vitality (obs_v1 input, 3.1) — monotonic Weibull decay model.
    pub vitality: f64,
    /// 6.1: remembered food locations as `[x, y, strength]` (viewer memory
    /// dots). Strength fades toward 0 as the memory decays (4.2).
    pub memory: Vec<[f64; 3]>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PredatorSnapshot {
    pub uid: String,
    pub pos: [f64; 2],
    pub lifetime_remaining_s: f64,
    /// Sprint 2 (Audit 5, B20): hunt state as a compact enum discriminant.
    pub hunt_state: components::PredatorHuntState,
    /// 6.2: dynamic speed tier on the 1 (slow)..5 (very fast) scale.
    pub speed_level: u8,
    /// 6.2: captures eaten so far.
    pub meals_eaten: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ObstacleSnapshot {
    pub id: u32,
    pub kind: ObstacleKind,
    pub min: [f64; 2],
    pub max: [f64; 2],
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    pub frame: u32,
    pub time_us: u64,
    pub light_level: f64,
    /// 4.4: current weather state + blend intensity for the viewer.
    /// Sprint 2 (Audit 5, B20): compact enum discriminant (serde encodes it as
    /// the variant name string, so JSON is unchanged).
    pub weather: components::Weather,
    pub weather_intensity: f64,
    pub agents: Vec<AgentSnapshot>,
    pub grains: Vec<[f64; 2]>, // Naprawiono zagnieżdżenie (Ticket R2-10)
    pub predators: Vec<PredatorSnapshot>,
    /// 4.3: static obstacles (buildings/trees/water) — static map geometry.
    pub obstacles: Vec<ObstacleSnapshot>,
    pub agent_count: usize,
    pub dead_count: u32,
}

/// Audit 3 (Phase 2) — cached visible-grain list for one agent. Stored on
/// `Simulation` (not a component) so it is readable inside the 15-component
/// agent query, and invalidated lazily by position/heading/range/version drift
/// (`GRAIN_VIS_CACHE_*_EPS`). Recomputed from the grain spatial index only when
/// stale, so O(agents × grains) visibility becomes O(agents × local grains)
/// amortized.
pub struct GrainVisCacheEntry {
    pub pos: Vector2<f64>,
    pub heading: f64,
    pub vision_range: f64,
    pub grains_version: u64,
    pub visible: Vec<(hecs::Entity, Vector2<f64>, u32)>,
}

/// Audit 3 (Phase 2) — cached neighbor SET for one agent (neighbor-query
/// memoization). Neighbor positions/velocities are always read fresh; only the
/// member set is throttled, refreshed every `NEIGHBOR_REFRESH_FRAMES` for dense,
/// stable flocks and every frame otherwise.
pub struct NeighborCacheEntry {
    pub neighbors: Vec<hecs::Entity>,
    pub last_count: usize,
    pub last_vel: Vector2<f64>,
}

pub struct Simulation {
    pub world: World,
    pub rng: rng::SimRng,
    pub time: time::SimulationTime,
    pub spatial_grid: spatial::SpatialHashGrid,
    pub physics: PhysicsWorld,
    pub config: SimulationConfig,
    pub environment: EnvironmentState,
    /// 4.3: static map obstacles (buildings/trees/water). Plain data, not
    /// world entities — see `Obstacle`.
    pub obstacles: Vec<Obstacle>,
    pub session_id: u32,
    pub next_uid: u64,
    pub deaths: u32,
    pub predator_kills: u32,
    /// 6.2: cumulative grains eaten (forage-success rate = grains_consumed /
    /// (arena area × elapsed seconds)).
    pub grains_consumed: u64,
    /// 6.2: age in years at death, one entry per despawned agent — fed to the
    /// survival-curve histogram in the metrics dashboard.
    pub death_ages: Vec<f64>,
    /// 2.5: event journal — `(frame, event, application result)`. Sprint 5:
    /// records whether each event actually applied (no-ops are NOT success) and
    /// is bounded to `MAX_EVENTS_LOG` entries (drained into telemetry each tick).
    pub events_log: Vec<(u32, Event, events::EventOutcome)>,
    /// 7.2 energy-balance accounting (kJ). Inflow from grain consumption, the
    /// amount actually drained from live agents, and energy removed from the
    /// pool when an agent despawns. Conservation: `Δ(live pool) = intake −
    /// expenditure − lost_at_death` across a run.
    pub total_energy_intake_kj: f64,
    pub total_energy_expenditure_kj: f64,
    pub total_energy_lost_at_death_kj: f64,
    /// 7.2: energy carried in by immigration respawns (inflow, not intake).
    pub total_energy_inflow_spawn_kj: f64,
    // Audit 3 (Phase 2) — targeted caching for the 500-agent wall.
    /// Spatial index of live grains (rebuilt each tick: cleared + re-inserted
    /// so bucket capacity is retained). The agent grid is `spatial_grid`.
    pub grain_grid: spatial::SpatialHashGrid,
    /// Per-agent cached visible-grain lists (dirty-flag invalidated).
    pub grain_vis_cache: FxHashMap<hecs::Entity, GrainVisCacheEntry>,
    /// Per-agent cached neighbor sets (throttled query memoization).
    pub neighbor_cache: FxHashMap<hecs::Entity, NeighborCacheEntry>,
    /// Monotonic counter bumped when a grain spawns or despawns — the dirty
    /// signal for `grain_vis_cache` so consumed/expired grains invalidate it.
    pub grains_version: u64,
    // Phase 9 (Audit 3) — emergent aerodynamics.
    /// Invisible updraft zones on the sun-facing sides of Buildings, re-derived
    /// every tick from `(obstacles, environment.sun_heading)`. Plain derived
    /// data (never serialized) — restored checkpoints recompute it identically.
    pub thermals: Vec<ThermalZone>,
}

impl Simulation {
    /// Sprint 1 (Audit 5): fallible constructor — validates the config first
    /// and returns a structural `ConfigError` instead of ever starting a
    /// broken simulation. See `validate()` for the rejected invariants.
    pub fn try_new(seed: u64, config: SimulationConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self::new_unchecked(seed, config))
    }

    /// Build an empty-arena simulation. `seed` is the explicit per-run seed;
    /// `config.seed` (if any) is ignored here — see `from_config`.
    ///
    /// Sprint 1: refuses to start with an invalid config. Prefer
    /// `try_new`/`try_from_config` where the caller wants the error instead of
    /// a panic.
    pub fn new(seed: u64, config: SimulationConfig) -> Self {
        match Self::try_new(seed, config) {
            Ok(sim) => sim,
            Err(e) => panic!("{e}"),
        }
    }

    fn new_unchecked(seed: u64, config: SimulationConfig) -> Self {
        let (w, h) = (config.world_width as f32, config.world_height as f32);
        let mut physics = PhysicsWorld::new();
        // Sprint 1: SimulationConfig::dt and gravity actually reach Rapier's
        // IntegrationParameters + gravity vector (they were hard-coded before).
        physics.set_dt(config.dt);
        physics.set_gravity(nalgebra::Vector2::new(0.0, config.gravity as f32));
        physics.add_wall(
            nalgebra::Vector2::new(0.0, 0.0),
            nalgebra::Vector2::new(w, 0.0),
        );
        physics.add_wall(nalgebra::Vector2::new(w, 0.0), nalgebra::Vector2::new(w, h));
        physics.add_wall(nalgebra::Vector2::new(w, h), nalgebra::Vector2::new(0.0, h));
        physics.add_wall(
            nalgebra::Vector2::new(0.0, h),
            nalgebra::Vector2::new(0.0, 0.0),
        );

        let mut sim = Self {
            world: World::new(),
            rng: rng::SimRng::from_seed(seed),
            time: time::SimulationTime::new(config.dt),
            // Sprint 2 (Audit 5, B22): pre-size the spatial buckets to the
            // arena area / cell² so the grid never rehashes as agents spawn in.
            spatial_grid: spatial::SpatialHashGrid::with_capacity(
                2.0,
                ((config.world_width / 2.0).ceil() as usize)
                    * ((config.world_height / 2.0).ceil() as usize),
            ),
            physics,
            config,
            environment: EnvironmentState::default(),
            obstacles: Vec::new(),
            session_id: 1,
            next_uid: 1,
            deaths: 0,
            predator_kills: 0,
            grains_consumed: 0,
            death_ages: Vec::new(),
            events_log: Vec::new(),
            total_energy_intake_kj: 0.0,
            total_energy_expenditure_kj: 0.0,
            total_energy_lost_at_death_kj: 0.0,
            total_energy_inflow_spawn_kj: 0.0,
            grain_grid: spatial::SpatialHashGrid::new(2.0),
            grain_vis_cache: FxHashMap::default(),
            neighbor_cache: FxHashMap::default(),
            grains_version: 0,
            thermals: Vec::new(),
        };
        // 5.2: an explicit scenario layout beats the built-in default map.
        let custom_specs = sim.config.obstacles.clone();
        if !custom_specs.is_empty() {
            for spec in &custom_specs {
                sim.add_obstacle(
                    spec.kind,
                    nalgebra::Vector2::new(spec.min[0], spec.min[1]),
                    nalgebra::Vector2::new(spec.max[0], spec.max[1]),
                );
            }
        // 4.3: opt-in default urban map (kept off by default so existing
        // deterministic test scenarios keep their exact trajectories).
        } else if sim.config.urban_obstacles {
            sim.build_default_obstacles();
        }
        sim
    }

    /// 5.2: construct from a toml-loaded `SimulationConfig`, using its `seed`
    /// field (falling back to 42 when absent).
    pub fn from_config(config: SimulationConfig) -> Self {
        match Self::try_from_config(config) {
            Ok(sim) => sim,
            Err(e) => panic!("{e}"),
        }
    }

    /// Sprint 1 (Audit 5): fallible variant of `from_config`.
    pub fn try_from_config(config: SimulationConfig) -> Result<Self, ConfigError> {
        let seed = config.seed.unwrap_or(42);
        Self::try_new(seed, config)
    }

    /// 4.3: register a static box obstacle (collider + data). Returns its id.
    pub fn add_obstacle(
        &mut self,
        kind: ObstacleKind,
        min: Vector2<f64>,
        max: Vector2<f64>,
    ) -> u32 {
        let id = self.obstacles.len() as u32;
        self.physics.add_obstacle(min, max);
        self.obstacles.push(Obstacle { id, kind, min, max });
        id
    }

    /// 4.3: the default urban map — three small buildings scattered across the
    /// 32×21 arena, sized so every corridor stays passable. Called from
    /// `Simulation::new` when `config.urban_obstacles` is set (and from
    /// `load_checkpoint` so restored runs rebuild the same map).
    pub fn build_default_obstacles(&mut self) {
        self.add_obstacle(
            ObstacleKind::Building,
            Vector2::new(6.0, 3.0),
            Vector2::new(10.0, 7.0),
        );
        self.add_obstacle(
            ObstacleKind::Building,
            Vector2::new(16.0, 8.0),
            Vector2::new(21.0, 11.0),
        );
        self.add_obstacle(
            ObstacleKind::Tree,
            Vector2::new(13.0, 16.0),
            Vector2::new(14.5, 18.0),
        );
        self.add_obstacle(
            ObstacleKind::Tree,
            Vector2::new(25.0, 14.0),
            Vector2::new(26.5, 15.5),
        );
        self.add_obstacle(
            ObstacleKind::Water,
            Vector2::new(7.0, 12.0),
            Vector2::new(11.0, 13.5),
        );
    }

    /// Phase 9 (Audit 3): re-derive the invisible building-thermal updraft
    /// zones from `(obstacles, environment.sun_heading)`. Only
    /// `ObstacleKind::Building` produces thermals; the sun-facing side is the
    /// edge whose outward normal most aligns with the sun direction. The zone
    /// is a `THERMAL_DEPTH_M` strip just outside that edge; `flow` is the
    /// axis-aligned updraft/airflow along the face (vertical faces rise +y,
    /// horizontal faces run +x). Deterministic: obstacles + sun are both plain
    /// data, so restored checkpoints recompute identical zones.
    pub fn update_thermals(&mut self) {
        self.thermals.clear();
        let sun_heading = self.environment.sun_heading;
        let sun_dir = Vector2::new(sun_heading.cos(), sun_heading.sin());
        let d = calibration::THERMAL_DEPTH_M;
        for o in &self.obstacles {
            if o.kind != ObstacleKind::Building {
                continue;
            }
            // (normal, strip rect, flow) for each face. Sun-facing = max
            // dot(outward normal, sun direction).
            let candidates = [
                // East face (right edge), flow rises +y along the wall.
                (
                    Vector2::new(1.0, 0.0),
                    Vector2::new(o.max.x, o.min.y),
                    Vector2::new(o.max.x + d, o.max.y),
                    Vector2::new(0.0, 1.0),
                ),
                // North face (top edge), flow runs +x along the wall.
                (
                    Vector2::new(0.0, 1.0),
                    Vector2::new(o.min.x, o.max.y),
                    Vector2::new(o.max.x, o.max.y + d),
                    Vector2::new(1.0, 0.0),
                ),
                // West face (left edge), flow rises -y along the wall.
                (
                    Vector2::new(-1.0, 0.0),
                    Vector2::new(o.min.x - d, o.min.y),
                    Vector2::new(o.min.x, o.max.y),
                    Vector2::new(0.0, -1.0),
                ),
                // South face (bottom edge), flow runs -x along the wall.
                (
                    Vector2::new(0.0, -1.0),
                    Vector2::new(o.min.x, o.min.y - d),
                    Vector2::new(o.max.x, o.min.y),
                    Vector2::new(-1.0, 0.0),
                ),
            ];
            let (_, min, max, flow) = candidates
                .iter()
                .max_by(|a, b| {
                    let da = a.0.dot(&sun_dir);
                    let db = b.0.dot(&sun_dir);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();
            self.thermals.push(ThermalZone {
                min: *min,
                max: *max,
                flow: *flow,
            });
        }
    }

    /// 4.3: true if `p` lies strictly inside any obstacle's bounding box.
    /// Used to keep spawns, immigration and predator patrol waypoints out of
    /// walls/buildings (the physics collider would otherwise pin them there).
    pub fn point_in_obstacles(obstacles: &[Obstacle], p: Vector2<f64>) -> bool {
        obstacles
            .iter()
            .any(|o| p.x >= o.min.x && p.x <= o.max.x && p.y >= o.min.y && p.y <= o.max.y)
    }

    /// 4.3/5.2: a random point in the interior of the `w × h` arena that is NOT
    /// inside any obstacle. Samples up to `MAX_FREE_POINT_TRIES` candidates and
    /// returns `None` if every draw lands inside an obstacle (Sprint 2: the old
    /// code silently fell back to the last — possibly obstructed — point, which
    /// could pin a spawn inside a collider).
    pub fn random_free_point(
        w: f64,
        h: f64,
        obstacles: &[Obstacle],
        rng: &mut rng::SimRng,
    ) -> Option<Vector2<f64>> {
        for _ in 0..calibration::MAX_FREE_POINT_TRIES {
            let p = Vector2::new(rng.gen_range(2.0..w - 2.0), rng.gen_range(2.0..h - 2.0));
            if !Self::point_in_obstacles(obstacles, p) {
                return Some(p);
            }
        }
        None
    }

    /// 7.2: total energy currently held by live agents (kJ).
    pub fn total_live_energy_kj(&self) -> f64 {
        self.world
            .query::<&Metabolism>()
            .iter()
            .fold(0.0, |acc, (_, m)| acc + m.energy_kj)
    }

    /// 3.3: allocate the next stable entity UID — `A{session:04}-{id:06}`.
    pub fn next_uid_str(&mut self) -> String {
        let uid = format!("A{:04}-{:06}", self.session_id, self.next_uid);
        self.next_uid += 1;
        uid
    }

    /// Find an agent (has Metabolism) by stable UID.
    pub fn find_agent_uid(&self, uid: &str) -> Option<hecs::Entity> {
        self.world
            .query::<(&AgentUid, &Metabolism)>()
            .iter()
            .find(|(_, (a, _))| a.0 == uid)
            .map(|(e, _)| e)
    }

    /// Find a predator by stable UID.
    pub fn find_predator_uid(&self, uid: &str) -> Option<hecs::Entity> {
        self.world
            .query::<(&AgentUid, &Predator)>()
            .iter()
            .find(|(_, (a, _))| a.0 == uid)
            .map(|(e, _)| e)
    }

    /// Spawn a grain entity (2.5 + existing spawn path).
    pub fn spawn_grain_entity(&mut self, pos: Vector2<f64>, amount: u32) -> hecs::Entity {
        // Audit 3 (Phase 2): bump the set version so per-agent visible-grain
        // caches are invalidated (this covers direct spawns AND SpawnGrain
        // events, which route through here).
        self.grains_version = self.grains_version.wrapping_add(1);
        self.world.spawn((Position(pos), Grain { amount }))
    }

    /// Spawn a predator entity (2.2/2.5). Its lifetime is a random draw in
    /// `[PREDATOR_LIFETIME_MIN_S, PREDATOR_LIFETIME_MAX_S]` (2.2b).
    pub fn spawn_predator(&mut self, pos: Vector2<f64>) -> hecs::Entity {
        let handle = self
            .physics
            .spawn_predator_body(nalgebra::Vector2::new(pos.x as f32, pos.y as f32), 1.0);
        let uid = self.next_uid_str();
        // 2.2b: randomized 5-15 s lifetime (config-gated for headless tests).
        let lifetime = if self.config.predator_expiry {
            self.rng.gen_range(
                calibration::PREDATOR_LIFETIME_MIN_S..=calibration::PREDATOR_LIFETIME_MAX_S,
            )
        } else {
            f64::INFINITY
        };
        self.world.spawn((
            Position(pos),
            Velocity(Vector2::zeros()),
            Predator {
                speed_multiplier: calibration::PREDATOR_SPEED_MULTIPLIER,
                detection_radius: calibration::PREDATOR_DETECTION_RADIUS_M,
                capture_cooldown: 0,
                patrol_target: None,
                lifetime_remaining_s: lifetime,
                meals_eaten: 0,
                hunt_state: PredatorHuntState::Await,
                speed_level: calibration::PREDATOR_SPEED_LEVEL_MIN,
                hunt_timer_s: 0.0,
            },
            AgentUid(uid),
            PhysicsHandle(handle),
        ))
    }

    /// 2.5: inject an RLHF control event. Each event is logged with the current
    /// frame so it appears as a ground-truth annotation in telemetry, and with
    /// its application result (Sprint 5): an event that matched nothing (no-op)
    /// is recorded as `NoOp`, never reported as success.
    pub fn inject_event(&mut self, event: Event) -> events::EventOutcome {
        let frame = self.time.frame;
        let outcome = self.apply_event(&event);
        // Bound the journal: keep only the most recent entries (telemetry
        // drains it every tick, so this only matters for a paused run).
        self.events_log.push((frame, event.clone(), outcome));
        if self.events_log.len() > Self::MAX_EVENTS_LOG {
            let excess = self.events_log.len() - Self::MAX_EVENTS_LOG;
            self.events_log.drain(..excess);
        }
        outcome
    }

    /// The maximum number of events retained in `events_log` between telemetry
    /// drains. Prevents the journal growing without bound on a paused run.
    pub const MAX_EVENTS_LOG: usize = 4096;

    /// Apply an event and report whether it changed state (`Applied`) or
    /// matched nothing (`NoOp`).
    fn apply_event(&mut self, event: &Event) -> events::EventOutcome {
        match event {
            Event::SpawnGrain(req) => {
                self.spawn_grain_entity(Vector2::new(req.pos[0], req.pos[1]), req.count);
                events::EventOutcome::Applied
            }
            Event::SpawnPredator(req) => {
                self.spawn_predator(Vector2::new(req.pos[0], req.pos[1]));
                events::EventOutcome::Applied
            }
            Event::RemovePredator(req) => {
                if let Some(id) = self.find_predator_uid(&req.uid) {
                    let handle = self.world.get::<&PhysicsHandle>(id).ok().map(|h| h.0);
                    self.world.despawn(id).ok();
                    if let Some(h) = handle {
                        self.physics.remove_body(h);
                    }
                    events::EventOutcome::Applied
                } else {
                    events::EventOutcome::NoOp
                }
            }
            Event::SetWeather(req) => {
                self.environment.weather = req.weather;
                // 4.4: entering Wind picks a fresh global wind direction so
                // event-forced weather behaves like scheduler-rolled weather.
                if req.weather == Weather::Wind {
                    self.environment.wind_heading = self.rng.gen_range(0.0..std::f64::consts::TAU);
                }
                events::EventOutcome::Applied
            }
            Event::TeleportAgent(req) => {
                let mut found = false;
                if let Some(id) = self.find_agent_uid(&req.uid) {
                    if let Ok(mut pos) = self.world.get::<&mut Position>(id) {
                        pos.0 = Vector2::new(req.pos[0], req.pos[1]);
                    }
                    if let Ok(h) = self.world.get::<&PhysicsHandle>(id) {
                        if let Some(rb) = self.physics.get_body_mut(h.0) {
                            rb.set_translation(
                                nalgebra::Vector2::new(req.pos[0] as f32, req.pos[1] as f32),
                                true,
                            );
                        }
                    }
                    found = true;
                }
                if found {
                    events::EventOutcome::Applied
                } else {
                    events::EventOutcome::NoOp
                }
            }
            Event::KillAgent(req) => {
                if let Some(id) = self.find_agent_uid(&req.uid) {
                    let handle = self.world.get::<&PhysicsHandle>(id).ok().map(|h| h.0);
                    self.world.despawn(id).ok();
                    if let Some(h) = handle {
                        self.physics.remove_body(h);
                    }
                    self.deaths += 1;
                    events::EventOutcome::Applied
                } else {
                    events::EventOutcome::NoOp
                }
            }
        }
    }

    pub fn step<F: FnMut(&mut Simulation, f64)>(&mut self, mut tick_fn: F) {
        self.time.tick();
        while self.time.consume_tick() {
            self.time.frame += 1;
            tick_fn(self, self.config.dt);
        }
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        // Sprint 2 (Audit 5, B18): the memory slots are read in the SAME query
        // (hecs supports 15-element tuples) — the old code built a separate
        // `HashMap<Entity, Vec<[f64;3]>>` per snapshot and then CLONED the
        // vector again per agent, i.e. two heap copies of every memory list.
        // FSM/hunt-state/weather now use `as_str()` instead of `format!("{:?}")`
        // so the snapshot path performs no per-agent string formatting.
        let mut agents = Vec::new();
        for (_id, (pos, head, vel, meta, mass, age, fsm, hb, uid, alarm, memory)) in self
            .world
            .query::<(
                &Position,
                &Heading,
                &Velocity,
                &Metabolism,
                &Mass,
                &Age,
                &FSMState,
                &HeadBob,
                &AgentUid,
                &Alarm,
                &MemorySlots,
            )>()
            .iter()
        {
            agents.push(AgentSnapshot {
                uid: uid.0.clone(),
                pos: [pos.0.x, pos.0.y],
                heading: head.0,
                vel: [vel.0.x, vel.0.y],
                mass_g: mass.current_g,
                age_years: age.years,
                energy_kj: meta.energy_kj,
                hunger: meta.hunger,
                fsm_state: *fsm,
                head_offset: [hb.offset.x, hb.offset.y],
                alarm_triggered: alarm.0,
                sick: age.vitality < calibration::SICK_VITALITY_THRESHOLD,
                vitality: age.vitality,
                // 6.1: viewer memory dots — [x, y, strength] of remembered food.
                memory: memory
                    .slots
                    .iter()
                    .map(|s| [s.pos.x, s.pos.y, s.strength])
                    .collect(),
            });
        }

        let mut grains = Vec::new();
        for (_id, (pos, grain)) in self.world.query::<(&Position, &Grain)>().iter() {
            if grain.amount > 0 {
                grains.push([pos.0.x, pos.0.y]);
            }
        }

        let mut predators = Vec::new();
        for (_id, (pos, pred, uid)) in self
            .world
            .query::<(&Position, &Predator, &AgentUid)>()
            .iter()
        {
            predators.push(PredatorSnapshot {
                uid: uid.0.clone(),
                pos: [pos.0.x, pos.0.y],
                lifetime_remaining_s: pred.lifetime_remaining_s,
                hunt_state: pred.hunt_state,
                speed_level: pred.speed_level,
                meals_eaten: pred.meals_eaten,
            });
        }

        SimulationSnapshot {
            frame: self.time.frame,
            time_us: self.time.time_us,
            light_level: self.environment.light_level,
            weather: self.environment.weather,
            weather_intensity: self.environment.weather_intensity,
            agent_count: agents.len(),
            agents,
            grains,
            predators,
            obstacles: self
                .obstacles
                .iter()
                .map(|o| ObstacleSnapshot {
                    id: o.id,
                    kind: o.kind,
                    min: [o.min.x, o.min.y],
                    max: [o.max.x, o.max.y],
                })
                .collect(),
            dead_count: self.deaths,
        }
    }

    /// 3.6: write a full checkpoint (world + RNG + time + physics + counters)
    /// to `path` in bincode format. See `checkpoint::build_checkpoint`.
    ///
    /// Sprint 5 (B16): the write is ATOMIC — the checkpoint is serialized to a
    /// temp file, flushed to disk, then `rename`d over the target. An
    /// interrupted write (crash, power loss, disk-full) leaves the previous
    /// valid checkpoint intact instead of a truncated file at `path`.
    ///
    /// Sprint 5 (B16, release gate): the payload is prefixed with an
    /// 8-byte magic + a 64-bit FNV-1a checksum over the bincode bytes, so a
    /// corrupted-but-structurally-valid file (bit flips in padding, etc.) is
    /// rejected on load instead of silently restoring wrong state. bincode is
    /// not self-describing — without the checksum random corruption in
    /// non-structural regions can deserialize cleanly.
    pub fn save_checkpoint(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let ckpt = checkpoint::build_checkpoint(self)?;
        let bytes = bincode::serialize(&ckpt)?;
        let mut header = Vec::with_capacity(16 + bytes.len());
        header.extend_from_slice(checkpoint::CHECKPOINT_MAGIC);
        header.extend_from_slice(&checkpoint::checksum_fnv1a(&bytes).to_le_bytes());
        header.extend_from_slice(&bytes);
        let tmp = format!("{path}.tmp");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&header)?;
            f.flush()?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 3.6: restore a full checkpoint written by `save_checkpoint`. The
    /// spatial grid is derived state and is rebuilt on the next tick, so it is
    /// intentionally not restored here. Corrupt/truncated files (including a
    /// wrong checksum) surface as `Err`, never a partial simulation.
    pub fn load_checkpoint(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = std::fs::read(path)?;
        if bytes.len() < 16 || &bytes[..8] != checkpoint::CHECKPOINT_MAGIC {
            return Err("checkpoint: missing magic header (not a v5+ checkpoint)".into());
        }
        let stored_checksum = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let payload = &bytes[16..];
        if checkpoint::checksum_fnv1a(payload) != stored_checksum {
            return Err("checkpoint: checksum mismatch (file corrupted or truncated)".into());
        }
        let ckpt: checkpoint::Checkpoint = bincode::deserialize(payload)?;
        if ckpt.version != checkpoint::CHECKPOINT_VERSION {
            return Err(format!(
                "checkpoint version {} != expected {}",
                ckpt.version,
                checkpoint::CHECKPOINT_VERSION
            )
            .into());
        }
        let world = checkpoint::deserialize_world(&ckpt.world_bytes)?;
        let physics = avian_physics::PhysicsWorld::from_state(ckpt.physics);
        let mut sim = Self {
            world,
            rng: ckpt.rng,
            time: ckpt.time,
            spatial_grid: spatial::SpatialHashGrid::new(2.0),
            physics,
            config: ckpt.config,
            environment: ckpt.environment,
            session_id: ckpt.session_id,
            next_uid: ckpt.next_uid,
            deaths: ckpt.deaths,
            predator_kills: ckpt.predator_kills,
            grains_consumed: ckpt.grains_consumed,
            death_ages: ckpt.death_ages,
            events_log: ckpt.events_log,
            total_energy_intake_kj: ckpt.total_energy_intake_kj,
            total_energy_expenditure_kj: ckpt.total_energy_expenditure_kj,
            total_energy_lost_at_death_kj: ckpt.total_energy_lost_at_death_kj,
            total_energy_inflow_spawn_kj: ckpt.total_energy_inflow_spawn_kj,
            // 4.3: obstacles are plain data — restored straight from the
            // checkpoint. The matching fixed colliders already live inside the
            // restored `physics` state, so nothing is re-added here.
            obstacles: ckpt.obstacles,
            // Audit 3 (Phase 2): transient caches are restored below (bit-exact
            // continuation); these placeholders are overwritten by the remap.
            grain_grid: spatial::SpatialHashGrid::new(2.0),
            grain_vis_cache: FxHashMap::default(),
            neighbor_cache: FxHashMap::default(),
            grains_version: ckpt.grains_version,
            thermals: Vec::new(),
        };
        // Rebuild the spatial grid so it matches world positions immediately.
        sim.rebuild_spatial_grid();

        // Audit 3 (Phase 2): remap the serialized phase-2 caches through the
        // world-query ordinals (preserved 1:1 by the column round-trip).
        let mut agent_by_ord: FxHashMap<usize, hecs::Entity> = FxHashMap::default();
        for (i, (e, _)) in sim
            .world
            .query::<(&AgentUid, &Metabolism)>()
            .iter()
            .enumerate()
        {
            agent_by_ord.insert(i, e);
        }
        let mut grain_by_ord: FxHashMap<usize, hecs::Entity> = FxHashMap::default();
        for (i, (e, _)) in sim.world.query::<&Grain>().iter().enumerate() {
            grain_by_ord.insert(i, e);
        }
        for nc in &ckpt.neighbor_cache {
            if let Some(e) = agent_by_ord.get(&nc.agent) {
                sim.neighbor_cache.insert(
                    *e,
                    NeighborCacheEntry {
                        neighbors: nc
                            .neighbors
                            .iter()
                            .filter_map(|o| agent_by_ord.get(o).copied())
                            .collect(),
                        last_count: nc.last_count,
                        last_vel: Vector2::new(nc.last_vel[0], nc.last_vel[1]),
                    },
                );
            }
        }
        for gc in &ckpt.grain_vis_cache {
            if let Some(e) = agent_by_ord.get(&gc.agent) {
                sim.grain_vis_cache.insert(
                    *e,
                    GrainVisCacheEntry {
                        pos: Vector2::new(gc.pos[0], gc.pos[1]),
                        heading: gc.heading,
                        vision_range: gc.vision_range,
                        grains_version: gc.grains_version,
                        visible: gc
                            .visible
                            .iter()
                            .filter_map(|(o, p, amt)| {
                                grain_by_ord
                                    .get(o)
                                    .map(|ge| (*ge, Vector2::new(p[0], p[1]), *amt))
                            })
                            .collect(),
                    },
                );
            }
        }
        Ok(sim)
    }

    /// Rebuild the spatial grid from current `Position` components (used after
    /// checkpoint load; the grid is normally refreshed every tick anyway).
    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();
        for (id, pos) in self.world.query::<&Position>().iter() {
            if self.world.get::<&Velocity>(id).is_ok() && self.world.get::<&Metabolism>(id).is_ok()
            {
                self.spatial_grid.insert(id, pos.0);
            }
        }
    }
}
