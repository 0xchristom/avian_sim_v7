pub mod time;
pub mod rng;
pub mod spatial;
pub mod components;
pub mod calibration;
pub mod events;
pub mod checkpoint;

use hecs::World;
use serde::{Serialize, Deserialize};
use components::*;
use events::Event;
use avian_physics::PhysicsWorld;
use nalgebra::Vector2;

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
            immigration_enabled: true,
            urban_obstacles: false,
            weather_enabled: false,
            seed: None,
            world_width: calibration::WORLD_WIDTH_M,
            world_height: calibration::WORLD_HEIGHT_M,
            initial_agents: 30,
            initial_grains: 15,
            time_scale: 1.0,
            foraging_threshold: calibration::FORAGING_HUNGER_THRESHOLD,
            flocking_enabled: true,
            obstacles: Vec::new(),
            event_schedule: Vec::new(),
        }
    }
}

impl SimulationConfig {
    /// 5.2: read a `simulation.toml` file. Any field the file omits falls back
    /// to the compiled default — biology constants stay in `calibration.rs`,
    /// only scenario params belong in the file (see DEVELOPMENT_PLAN §5.2).
    pub fn from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let s = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&s)?)
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
    pub fsm_state: String,
    pub head_offset: [f64; 2],
    pub alarm_triggered: bool,
    /// 2.7 anomaly ground-truth label — vitality below SICK_VITALITY_THRESHOLD.
    pub sick: bool,
    /// 4.0 vitality (obs_v1 input, 3.1) — monotonic Weibull decay model.
    pub vitality: f64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PredatorSnapshot {
    pub uid: String,
    pub pos: [f64; 2],
    pub lifetime_remaining_s: f64,
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
    pub weather: String,
    pub weather_intensity: f64,
    pub agents: Vec<AgentSnapshot>,
    pub grains: Vec<[f64; 2]>, // Naprawiono zagnieżdżenie (Ticket R2-10)
    pub predators: Vec<PredatorSnapshot>,
    /// 4.3: static obstacles (buildings/trees/water) — static map geometry.
    pub obstacles: Vec<ObstacleSnapshot>,
    pub agent_count: usize,
    pub dead_count: u32,
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
    pub events_log: Vec<(u32, Event)>,
    /// 7.2 energy-balance accounting (kJ). Inflow from grain consumption, the
    /// amount actually drained from live agents, and energy removed from the
    /// pool when an agent despawns. Conservation: `Δ(live pool) = intake −
    /// expenditure − lost_at_death` across a run.
    pub total_energy_intake_kj: f64,
    pub total_energy_expenditure_kj: f64,
    pub total_energy_lost_at_death_kj: f64,
    /// 7.2: energy carried in by immigration respawns (inflow, not intake).
    pub total_energy_inflow_spawn_kj: f64,
}

impl Simulation {
    /// Build an empty-arena simulation. `seed` is the explicit per-run seed;
    /// `config.seed` (if any) is ignored here — see `from_config`.
    pub fn new(seed: u64, config: SimulationConfig) -> Self {
        let (w, h) = (config.world_width as f32, config.world_height as f32);
        let mut physics = PhysicsWorld::new();
        physics.add_wall(nalgebra::Vector2::new(0.0, 0.0), nalgebra::Vector2::new(w, 0.0));
        physics.add_wall(nalgebra::Vector2::new(w, 0.0), nalgebra::Vector2::new(w, h));
        physics.add_wall(nalgebra::Vector2::new(w, h), nalgebra::Vector2::new(0.0, h));
        physics.add_wall(nalgebra::Vector2::new(0.0, h), nalgebra::Vector2::new(0.0, 0.0));

        let mut sim = Self {
            world: World::new(),
            rng: rng::SimRng::from_seed(seed),
            time: time::SimulationTime::new(config.dt),
            spatial_grid: spatial::SpatialHashGrid::new(2.0),
            physics,
            config,
            environment: EnvironmentState::default(),
            obstacles: Vec::new(),
            session_id: 1,
            next_uid: 1,
            deaths: 0,
            predator_kills: 0,
            events_log: Vec::new(),
            total_energy_intake_kj: 0.0,
            total_energy_expenditure_kj: 0.0,
            total_energy_lost_at_death_kj: 0.0,
            total_energy_inflow_spawn_kj: 0.0,
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
        Self::new(config.seed.unwrap_or(42), config)
    }

    /// 4.3: register a static box obstacle (collider + data). Returns its id.
    pub fn add_obstacle(&mut self, kind: ObstacleKind, min: Vector2<f64>, max: Vector2<f64>) -> u32 {
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
        self.add_obstacle(ObstacleKind::Building, Vector2::new(6.0, 3.0), Vector2::new(10.0, 7.0));
        self.add_obstacle(ObstacleKind::Building, Vector2::new(16.0, 8.0), Vector2::new(21.0, 11.0));
        self.add_obstacle(ObstacleKind::Tree, Vector2::new(13.0, 16.0), Vector2::new(14.5, 18.0));
        self.add_obstacle(ObstacleKind::Tree, Vector2::new(25.0, 14.0), Vector2::new(26.5, 15.5));
        self.add_obstacle(ObstacleKind::Water, Vector2::new(7.0, 12.0), Vector2::new(11.0, 13.5));
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
    /// inside any obstacle. Samples up to `MAX_FREE_POINT_TRIES` candidates
    /// before giving up and returning the last (non-obstructed) draw.
    pub fn random_free_point(
        w: f64,
        h: f64,
        obstacles: &[Obstacle],
        rng: &mut rng::SimRng,
    ) -> Vector2<f64> {
        let mut p = Vector2::new(0.0, 0.0);
        for _ in 0..calibration::MAX_FREE_POINT_TRIES {
            p = Vector2::new(rng.gen_range(2.0..w - 2.0), rng.gen_range(2.0..h - 2.0));
            if !Self::point_in_obstacles(obstacles, p) {
                return p;
            }
        }
        p
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
        self.world.spawn((Position(pos), Grain { amount }))
    }

    /// Spawn a predator entity (2.2/2.5). Its lifetime is a random draw in
    /// `[PREDATOR_LIFETIME_MIN_S, PREDATOR_LIFETIME_MAX_S]` (2.2b).
    pub fn spawn_predator(&mut self, pos: Vector2<f64>) -> hecs::Entity {
        let handle = self.physics.spawn_predator_body(
            nalgebra::Vector2::new(pos.x as f32, pos.y as f32),
            1.0,
        );
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
            },
            AgentUid(uid),
            PhysicsHandle(handle),
        ))
    }

    /// 2.5: inject an RLHF control event. Each event is logged with the current
    /// frame so it appears as a ground-truth annotation in telemetry.
    pub fn inject_event(&mut self, event: Event) {
        let frame = self.time.frame;
        self.events_log.push((frame, event.clone()));
        match event {
            Event::SpawnGrain(req) => {
                self.spawn_grain_entity(Vector2::new(req.pos[0], req.pos[1]), req.count);
            }
            Event::SpawnPredator(req) => {
                self.spawn_predator(Vector2::new(req.pos[0], req.pos[1]));
            }
            Event::RemovePredator(req) => {
                if let Some(id) = self.find_predator_uid(&req.uid) {
                    let handle = self.world.get::<&PhysicsHandle>(id).ok().map(|h| h.0);
                    self.world.despawn(id).ok();
                    if let Some(h) = handle {
                        self.physics.remove_body(h);
                    }
                }
            }
            Event::SetWeather(req) => {
                self.environment.weather = req.weather;
                // 4.4: entering Wind picks a fresh global wind direction so
                // event-forced weather behaves like scheduler-rolled weather.
                if req.weather == Weather::Wind {
                    self.environment.wind_heading = self.rng.gen_range(0.0..std::f64::consts::TAU);
                }
            }
            Event::TeleportAgent(req) => {
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
                }
            }
        }
    }

    pub fn step<F: FnMut(&mut Simulation, f64)>(&mut self, mut tick_fn: F) {
        self.time.tick();
        while self.time.consume_tick() {
            self.time.frame += 1;
            self.time.time_us += (self.config.dt * 1_000_000.0) as u64;
            tick_fn(self, self.config.dt);
        }
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        let mut agents = Vec::new();
        for (_id, (pos, head, vel, meta, mass, age, fsm, hb, uid, alarm)) in
            self.world.query::<(&Position, &Heading, &Velocity, &Metabolism, &Mass, &Age, &FSMState, &HeadBob, &AgentUid, &Alarm)>().iter()
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
                fsm_state: format!("{:?}", fsm),
                head_offset: [hb.offset.x, hb.offset.y],
                alarm_triggered: alarm.0,
                sick: age.vitality < calibration::SICK_VITALITY_THRESHOLD,
                vitality: age.vitality,
            });
        }

        let mut grains = Vec::new();
        for (_id, (pos, grain)) in self.world.query::<(&Position, &Grain)>().iter() {
            if grain.amount > 0 {
                grains.push([pos.0.x, pos.0.y]);
            }
        }

        let mut predators = Vec::new();
        for (_id, (pos, pred, uid)) in self.world.query::<(&Position, &Predator, &AgentUid)>().iter() {
            predators.push(PredatorSnapshot {
                uid: uid.0.clone(),
                pos: [pos.0.x, pos.0.y],
                lifetime_remaining_s: pred.lifetime_remaining_s,
            });
        }

        SimulationSnapshot {
            frame: self.time.frame,
            time_us: self.time.time_us,
            light_level: self.environment.light_level,
            weather: format!("{:?}", self.environment.weather),
            weather_intensity: self.environment.weather_intensity,
            agent_count: agents.len(),
            agents,
            grains,
            predators,
            obstacles: self.obstacles.iter().map(|o| ObstacleSnapshot {
                id: o.id,
                kind: o.kind,
                min: [o.min.x, o.min.y],
                max: [o.max.x, o.max.y],
            }).collect(),
            dead_count: self.deaths,
        }
    }

    /// 3.6: write a full checkpoint (world + RNG + time + physics + counters)
    /// to `path` in bincode format. See `checkpoint::build_checkpoint`.
    pub fn save_checkpoint(
        &self,
        path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let ckpt = checkpoint::build_checkpoint(self)?;
        let bytes = bincode::serialize(&ckpt)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// 3.6: restore a full checkpoint written by `save_checkpoint`. The
    /// spatial grid is derived state and is rebuilt on the next tick, so it is
    /// intentionally not restored here.
    pub fn load_checkpoint(
        path: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = std::fs::read(path)?;
        let ckpt: checkpoint::Checkpoint = bincode::deserialize(&bytes)?;
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
            events_log: ckpt.events_log,
            total_energy_intake_kj: ckpt.total_energy_intake_kj,
            total_energy_expenditure_kj: ckpt.total_energy_expenditure_kj,
            total_energy_lost_at_death_kj: ckpt.total_energy_lost_at_death_kj,
            total_energy_inflow_spawn_kj: ckpt.total_energy_inflow_spawn_kj,
            // 4.3: obstacles are plain data — restored straight from the
            // checkpoint. The matching fixed colliders already live inside the
            // restored `physics` state, so nothing is re-added here.
            obstacles: ckpt.obstacles,
        };
        // Rebuild the spatial grid so it matches world positions immediately.
        sim.rebuild_spatial_grid();
        Ok(sim)
    }

    /// Rebuild the spatial grid from current `Position` components (used after
    /// checkpoint load; the grid is normally refreshed every tick anyway).
    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();
        for (id, pos) in self.world.query::<&Position>().iter() {
            if self.world.get::<&Velocity>(id).is_ok() && self.world.get::<&Metabolism>(id).is_ok() {
                self.spatial_grid.insert(id, pos.0);
            }
        }
    }
}
