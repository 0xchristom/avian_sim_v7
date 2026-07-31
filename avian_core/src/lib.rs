pub mod time;
pub mod rng;
pub mod spatial;
pub mod components;

use hecs::World;
use serde::{Serialize, Deserialize};
use components::{Position, Heading, Metabolism, Mass, Age};

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
        }
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        let mut agents = Vec::new();
        // Pobieramy prawdziwe komponenty: Position, Heading, Metabolism, Mass, Age
        for (id, (pos, head, meta, mass, age)) in self.world.query::<(&Position, &Heading, &Metabolism, &Mass, &Age)>().iter() {
            agents.push(AgentSnapshot {
                uid: format!("A{:04}", id.to_bits().get() % 10000),
                pos: [pos.0.x, pos.0.y],
                heading: head.0,
                vel: [0.0, 0.0],
                mass_g: mass.current_g, // Prawdziwa waga
                age_years: age.years,   // Prawdziwy wiek
                energy_kj: meta.energy_kj,
                hunger: meta.hunger,
                fsm_state: "SPACER".to_string(),
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
    }
}
