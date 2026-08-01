pub mod time;
pub mod rng;
pub mod spatial;
pub mod components;

use hecs::World;
use serde::{Serialize, Deserialize};
use components::*;
use avian_physics::PhysicsWorld;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub dt: f64,
    pub gravity: f64,
    pub max_agents: usize,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self { dt: 1.0 / 120.0, gravity: 0.0, max_agents: 1000 }
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
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SimulationSnapshot {
    pub frame: u32,
    pub time_us: u64,
    pub agents: Vec<AgentSnapshot>,
    pub grains: Vec<[[f64; 2]; 1]>,
}

pub struct Simulation {
    pub world: World,
    pub rng: rng::SimRng,
    pub time: time::SimulationTime,
    pub spatial_grid: spatial::SpatialHashGrid,
    pub physics: PhysicsWorld,
    pub config: SimulationConfig,
}

impl Simulation {
    pub fn new(seed: u64, config: SimulationConfig) -> Self {
        let mut physics = PhysicsWorld::new();
        // Ściany świata (Ticket 6)
        physics.add_wall(nalgebra::Vector2::new(0.0, 0.0), nalgebra::Vector2::new(32.0, 0.0));
        physics.add_wall(nalgebra::Vector2::new(32.0, 0.0), nalgebra::Vector2::new(32.0, 21.0));
        physics.add_wall(nalgebra::Vector2::new(32.0, 21.0), nalgebra::Vector2::new(0.0, 21.0));
        physics.add_wall(nalgebra::Vector2::new(0.0, 21.0), nalgebra::Vector2::new(0.0, 0.0));

        Self {
            world: World::new(),
            rng: rng::SimRng::from_seed(seed),
            time: time::SimulationTime::new(config.dt),
            spatial_grid: spatial::SpatialHashGrid::new(2.0),
            physics,
            config,
        }
    }

    pub fn step<F: FnMut(&mut Simulation, f64)>(&mut self, mut tick_fn: F) {
        self.time.tick(); // Pojedyncza akumulacja
        
        while self.time.consume_tick() {
            self.time.frame += 1;
            self.time.time_us += (self.config.dt * 1_000_000.0) as u64;
            tick_fn(self, self.config.dt);
        }
    }

    pub fn snapshot(&self) -> SimulationSnapshot {
        let mut agents = Vec::new();
        for (id, (pos, head, vel, meta, mass, age, fsm, hb)) in self.world.query::<(&Position, &Heading, &Velocity, &Metabolism, &Mass, &Age, &FSMState, &HeadBob)>().iter() {
            agents.push(AgentSnapshot {
                uid: format!("A{:04}", id.to_bits().get() % 10000),
                pos: [pos.0.x, pos.0.y],
                heading: head.0,
                vel: [vel.0.x, vel.0.y],
                mass_g: mass.current_g,
                age_years: age.years,
                energy_kj: meta.energy_kj,
                hunger: meta.hunger,
                fsm_state: format!("{:?}", fsm),
                head_offset: [hb.offset.x, hb.offset.y],
            });
        }
        
        let mut grains = Vec::new();
        for (_id, (pos, _grain)) in self.world.query::<(&Position, &Grain)>().iter() {
            grains.push([[pos.0.x, pos.0.y]]);
        }

        SimulationSnapshot { frame: self.time.frame, time_us: self.time.time_us, agents, grains }
    }
}