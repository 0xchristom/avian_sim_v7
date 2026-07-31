pub mod time;
pub mod rng;
pub mod spatial;
pub mod components;

use hecs::World;
use serde::{Serialize, Deserialize};
use components::{Position, Heading, Metabolism, Mass, Age, Velocity};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub dt: f64,
    pub gravity: f64,
    pub max_agents: usize,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            dt: 1.0 / 120.0,
            gravity: 0.0,
            max_agents: 1000,
        }
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
    pub crop_count: u32,
    pub gizzard_count: u32,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SimulationSnapshot {
    pub frame: u32,
    pub time_us: u64,
    pub agents: Vec<AgentSnapshot>,
}

pub struct Simulation {
    pub world: World,
    pub rng: rng::SimRng,
    pub time: time::SimulationTime,
    pub spatial_grid: spatial::SpatialHashGrid,
    pub config: SimulationConfig,
}

impl Simulation {
    pub fn new(seed: u64, config: SimulationConfig) -> Self {
        Self {
            world: World::new(),
            rng: rng::SimRng::from_seed(seed),
            time: time::SimulationTime::new(config.dt),
            spatial_grid: spatial::SpatialHashGrid::new(2.0),
            config,
        }
    }

    pub fn step(&mut self) {
        self.time.tick();
        self.time.accumulator += self.config.dt;

        while self.time.consume_tick() {
            self.time.frame += 1;
            self.time.time_us += (self.config.dt * 1_000_000.0) as u64;
            
            // TODO: Tutaj wywołuj systemy ECS:
            // - spatial_grid.clear() + insert wszystkich agentów
            // - metabolism_system(&mut self.world, &self.time)
            // - locomotion_system(&mut self.world, &mut self.rng, &self.time)
            // - perception_system(&mut self.world, &self.spatial_grid)
            // - behavior_tree_system(&mut self.world, &mut self.rng, &self.time)
            // - physics_step (Rapier) — jeśli zostanie zintegrowane
        }
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        let mut agents = Vec::new();
        for (id, (pos, head, vel, meta, mass, age)) in self.world.query::<(&Position, &Heading, &Velocity, &Metabolism, &Mass, &Age)>().iter() {
            // Prosty heuristic FSM na podstawie stanu fizjologicznego
            let fsm = if meta.energy_kj < 5.0 {
                "IDLE"
            } else if meta.hunger > 0.7 {
                "FORAGING"
            } else if vel.0.norm() > 0.1 {
                "SPACER"
            } else {
                "IDLE"
            };
            
            agents.push(AgentSnapshot {
                uid: format!("A{:04}", id.to_bits().get() % 10000),
                pos: [pos.0.x, pos.0.y],
                heading: head.0,
                vel: [vel.0.x, vel.0.y],  // POPRAWKA: czytaj prawdziwą prędkość
                mass_g: mass.current_g,
                age_years: age.years,
                energy_kj: meta.energy_kj,
                hunger: meta.hunger,
                fsm_state: fsm.to_string(),  // POPRAWKA: heuristic FSM
                crop_count: meta.crop_count,
                gizzard_count: meta.gizzard_count,
            });
        }
        SimulationSnapshot {
            frame: self.time.frame,
            time_us: self.time.time_us,
            agents,
        }
    }

    pub fn load_snapshot(&mut self, snap: SimulationSnapshot) {
        self.time.frame = snap.frame;
        self.time.time_us = snap.time_us;
        // TODO: odtworzenie entity w ECS z snap.agents
    }
}