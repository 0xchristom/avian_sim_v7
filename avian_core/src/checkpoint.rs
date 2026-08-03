//! 3.6 Checkpoint/Replay — full simulation state serialization.
//!
//! Serializes: world (all entities + all components, archetype-preserving via
//! hecs column-serialize), RNG state (ChaCha8Rng via `serde1`), simulation
//! time, physics bodies/colliders (via `PhysicsState`), environment, and the
//! bookkeeping counters (UID counter, deaths, energy accounting).
//!
//! The spatial grid is NOT serialized: it is derived state, rebuilt from
//! `Position` components at the top of every `run_systems` tick, so restoring
//! it is both unnecessary and would bake in stale positions.
//!
//! Wire format: bincode (Rust-only, per plan line 424 — the JS decoder gap is
//! irrelevant because checkpoints never cross the WASM boundary).

use crate::components::*;
use crate::events::Event;
use crate::rng::SimRng;
use crate::time::SimulationTime;
use crate::SimulationConfig;
use avian_physics::PhysicsState;
use hecs::serialize::column::{
    deserialize_column, try_serialize, try_serialize_id, DeserializeContext, SerializeContext,
};
use hecs::World;
use hecs::{ColumnBatchBuilder, ColumnBatchType};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// One variant per component type registered in the world. Adding a new
/// component type to `spawn_agent`/`spawn_predator`/`spawn_grain_entity` MUST
/// be mirrored here and in both context impls below, or checkpoints silently
/// drop that column.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum ComponentId {
    Position,
    Velocity,
    Heading,
    Mass,
    Age,
    Metabolism,
    FSMState,
    LevyState,
    Mobility,
    Vision,
    HeadBob,
    FeatherCondition,
    PhysicsHandle,
    AgentUid,
    Grain,
    Predator,
    Alarm,
    AlarmPrev,
    MemorySlots,
}

/// Handles the 19 registered component types (agents, grains, predators).
struct WorldContext {
    /// Component IDs seen while deserializing the current archetype, in order.
    components: Vec<ComponentId>,
}

impl Default for WorldContext {
    fn default() -> Self {
        Self {
            components: Vec::with_capacity(19),
        }
    }
}

impl SerializeContext for WorldContext {
    fn component_count(&self, archetype: &hecs::Archetype) -> usize {
        use hecs::Archetype as A;
        [
            A::has::<Position>,
            A::has::<Velocity>,
            A::has::<Heading>,
            A::has::<Mass>,
            A::has::<Age>,
            A::has::<Metabolism>,
            A::has::<FSMState>,
            A::has::<LevyState>,
            A::has::<Mobility>,
            A::has::<Vision>,
            A::has::<HeadBob>,
            A::has::<FeatherCondition>,
            A::has::<PhysicsHandle>,
            A::has::<AgentUid>,
            A::has::<Grain>,
            A::has::<Predator>,
            A::has::<Alarm>,
            A::has::<AlarmPrev>,
            A::has::<MemorySlots>,
        ]
        .iter()
        .filter(|f| f(archetype))
        .count()
    }

    fn serialize_component_ids<S: serde::ser::SerializeTuple>(
        &mut self,
        archetype: &hecs::Archetype,
        mut out: S,
    ) -> Result<S::Ok, S::Error> {
        try_serialize_id::<Position, _, _>(archetype, &ComponentId::Position, &mut out)?;
        try_serialize_id::<Velocity, _, _>(archetype, &ComponentId::Velocity, &mut out)?;
        try_serialize_id::<Heading, _, _>(archetype, &ComponentId::Heading, &mut out)?;
        try_serialize_id::<Mass, _, _>(archetype, &ComponentId::Mass, &mut out)?;
        try_serialize_id::<Age, _, _>(archetype, &ComponentId::Age, &mut out)?;
        try_serialize_id::<Metabolism, _, _>(archetype, &ComponentId::Metabolism, &mut out)?;
        try_serialize_id::<FSMState, _, _>(archetype, &ComponentId::FSMState, &mut out)?;
        try_serialize_id::<LevyState, _, _>(archetype, &ComponentId::LevyState, &mut out)?;
        try_serialize_id::<Mobility, _, _>(archetype, &ComponentId::Mobility, &mut out)?;
        try_serialize_id::<Vision, _, _>(archetype, &ComponentId::Vision, &mut out)?;
        try_serialize_id::<HeadBob, _, _>(archetype, &ComponentId::HeadBob, &mut out)?;
        try_serialize_id::<FeatherCondition, _, _>(
            archetype,
            &ComponentId::FeatherCondition,
            &mut out,
        )?;
        try_serialize_id::<PhysicsHandle, _, _>(archetype, &ComponentId::PhysicsHandle, &mut out)?;
        try_serialize_id::<AgentUid, _, _>(archetype, &ComponentId::AgentUid, &mut out)?;
        try_serialize_id::<Grain, _, _>(archetype, &ComponentId::Grain, &mut out)?;
        try_serialize_id::<Predator, _, _>(archetype, &ComponentId::Predator, &mut out)?;
        try_serialize_id::<Alarm, _, _>(archetype, &ComponentId::Alarm, &mut out)?;
        try_serialize_id::<AlarmPrev, _, _>(archetype, &ComponentId::AlarmPrev, &mut out)?;
        try_serialize_id::<MemorySlots, _, _>(archetype, &ComponentId::MemorySlots, &mut out)?;
        out.end()
    }

    fn serialize_components<S: serde::ser::SerializeTuple>(
        &mut self,
        archetype: &hecs::Archetype,
        mut out: S,
    ) -> Result<S::Ok, S::Error> {
        try_serialize::<Position, _>(archetype, &mut out)?;
        try_serialize::<Velocity, _>(archetype, &mut out)?;
        try_serialize::<Heading, _>(archetype, &mut out)?;
        try_serialize::<Mass, _>(archetype, &mut out)?;
        try_serialize::<Age, _>(archetype, &mut out)?;
        try_serialize::<Metabolism, _>(archetype, &mut out)?;
        try_serialize::<FSMState, _>(archetype, &mut out)?;
        try_serialize::<LevyState, _>(archetype, &mut out)?;
        try_serialize::<Mobility, _>(archetype, &mut out)?;
        try_serialize::<Vision, _>(archetype, &mut out)?;
        try_serialize::<HeadBob, _>(archetype, &mut out)?;
        try_serialize::<FeatherCondition, _>(archetype, &mut out)?;
        try_serialize::<PhysicsHandle, _>(archetype, &mut out)?;
        try_serialize::<AgentUid, _>(archetype, &mut out)?;
        try_serialize::<Grain, _>(archetype, &mut out)?;
        try_serialize::<Predator, _>(archetype, &mut out)?;
        try_serialize::<Alarm, _>(archetype, &mut out)?;
        try_serialize::<AlarmPrev, _>(archetype, &mut out)?;
        try_serialize::<MemorySlots, _>(archetype, &mut out)?;
        out.end()
    }
}

impl DeserializeContext for WorldContext {
    fn deserialize_component_ids<'de, A>(&mut self, mut seq: A) -> Result<ColumnBatchType, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        self.components.clear();
        let mut batch = ColumnBatchType::new();
        while let Some(id) = seq.next_element()? {
            match id {
                ComponentId::Position => batch.add::<Position>(),
                ComponentId::Velocity => batch.add::<Velocity>(),
                ComponentId::Heading => batch.add::<Heading>(),
                ComponentId::Mass => batch.add::<Mass>(),
                ComponentId::Age => batch.add::<Age>(),
                ComponentId::Metabolism => batch.add::<Metabolism>(),
                ComponentId::FSMState => batch.add::<FSMState>(),
                ComponentId::LevyState => batch.add::<LevyState>(),
                ComponentId::Mobility => batch.add::<Mobility>(),
                ComponentId::Vision => batch.add::<Vision>(),
                ComponentId::HeadBob => batch.add::<HeadBob>(),
                ComponentId::FeatherCondition => batch.add::<FeatherCondition>(),
                ComponentId::PhysicsHandle => batch.add::<PhysicsHandle>(),
                ComponentId::AgentUid => batch.add::<AgentUid>(),
                ComponentId::Grain => batch.add::<Grain>(),
                ComponentId::Predator => batch.add::<Predator>(),
                ComponentId::Alarm => batch.add::<Alarm>(),
                ComponentId::AlarmPrev => batch.add::<AlarmPrev>(),
                ComponentId::MemorySlots => batch.add::<MemorySlots>(),
            };
            self.components.push(id);
        }
        Ok(batch)
    }

    fn deserialize_components<'de, A>(
        &mut self,
        entity_count: u32,
        mut seq: A,
        batch: &mut ColumnBatchBuilder,
    ) -> Result<(), A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        for component in &self.components {
            match *component {
                ComponentId::Position => {
                    deserialize_column::<Position, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Velocity => {
                    deserialize_column::<Velocity, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Heading => {
                    deserialize_column::<Heading, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Mass => deserialize_column::<Mass, _>(entity_count, &mut seq, batch)?,
                ComponentId::Age => deserialize_column::<Age, _>(entity_count, &mut seq, batch)?,
                ComponentId::Metabolism => {
                    deserialize_column::<Metabolism, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::FSMState => {
                    deserialize_column::<FSMState, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::LevyState => {
                    deserialize_column::<LevyState, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Mobility => {
                    deserialize_column::<Mobility, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Vision => {
                    deserialize_column::<Vision, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::HeadBob => {
                    deserialize_column::<HeadBob, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::FeatherCondition => {
                    deserialize_column::<FeatherCondition, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::PhysicsHandle => {
                    deserialize_column::<PhysicsHandle, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::AgentUid => {
                    deserialize_column::<AgentUid, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Grain => {
                    deserialize_column::<Grain, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Predator => {
                    deserialize_column::<Predator, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::Alarm => {
                    deserialize_column::<Alarm, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::AlarmPrev => {
                    deserialize_column::<AlarmPrev, _>(entity_count, &mut seq, batch)?
                }
                ComponentId::MemorySlots => {
                    deserialize_column::<MemorySlots, _>(entity_count, &mut seq, batch)?
                }
            }
        }
        Ok(())
    }
}

/// Audit 3 (Phase 2): checkpoint-safe form of the per-agent neighbor cache.
/// Agent references are stored as ordinals into the world's agent query order
/// (`query::<(&AgentUid, &Metabolism)>()`), which the column round-trip
/// preserves 1:1, so they remap cleanly to restored entities.
#[derive(Serialize, Deserialize, Clone)]
pub struct NeighborCacheSer {
    pub agent: usize,
    pub neighbors: Vec<usize>,
    pub last_count: usize,
    pub last_vel: [f64; 2],
}

/// Audit 3 (Phase 2): checkpoint-safe form of the per-agent visible-grain
/// cache. `agent` is an ordinal into the agent query order; `visible` entries
/// carry grain ordinals (into `query::<&Grain>()` order).
#[derive(Serialize, Deserialize, Clone)]
pub struct GrainVisCacheSer {
    pub agent: usize,
    pub pos: [f64; 2],
    pub heading: f64,
    pub vision_range: f64,
    pub grains_version: u64,
    pub visible: Vec<(usize, [f64; 2], u32)>,
}

/// Complete checkpoint: everything needed to resume a run exactly.
///
/// `world` is stored as bincode bytes (serialized separately through the
/// column-serialize context) so the struct stays `Serialize`-derivable.
#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: u32,
    pub config: SimulationConfig,
    pub rng: SimRng,
    pub time: SimulationTime,
    pub environment: crate::components::EnvironmentState,
    pub physics: PhysicsState,
    pub session_id: u32,
    pub next_uid: u64,
    pub deaths: u32,
    pub predator_kills: u32,
    /// 6.2: cumulative grains eaten + age-at-death list for the metrics
    /// dashboard (forage success rate, survival curve).
    pub grains_consumed: u64,
    pub death_ages: Vec<f64>,
    pub events_log: Vec<(u32, Event)>,
    pub total_energy_intake_kj: f64,
    pub total_energy_expenditure_kj: f64,
    pub total_energy_lost_at_death_kj: f64,
    pub total_energy_inflow_spawn_kj: f64,
    /// 4.3: static map obstacles. They are plain data (not world entities), so
    /// they travel in the checkpoint instead of a component column.
    pub obstacles: Vec<Obstacle>,
    pub world_bytes: Vec<u8>,
    /// Audit 3 (Phase 2): transient phase-2 caches + the grain-set version, so
    /// a continued run matches an un-checkpointed run bit-for-bit.
    pub grains_version: u64,
    pub neighbor_cache: Vec<NeighborCacheSer>,
    pub grain_vis_cache: Vec<GrainVisCacheSer>,
}

pub const CHECKPOINT_VERSION: u32 = 4;

/// Serialize the world column into a `Vec<u8>` via bincode.
pub fn serialize_world(world: &World) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut ctx = WorldContext::default();
    let mut buf = Vec::new();
    let mut serializer = bincode::Serializer::new(&mut buf, bincode::DefaultOptions::new());
    hecs::serialize::column::serialize(world, &mut ctx, &mut serializer)?;
    Ok(buf)
}

/// Deserialize a world column from bincode bytes.
pub fn deserialize_world(bytes: &[u8]) -> Result<World, Box<dyn std::error::Error>> {
    let mut ctx = WorldContext::default();
    let mut deserializer = bincode::Deserializer::from_slice(bytes, bincode::DefaultOptions::new());
    let world = hecs::serialize::column::deserialize(&mut ctx, &mut deserializer)?;
    Ok(world)
}

/// Build a complete checkpoint from a `Simulation` (borrows — world is
/// serialized into `world_bytes`, nothing is moved).
pub fn build_checkpoint(sim: &crate::Simulation) -> Result<Checkpoint, Box<dyn std::error::Error>> {
    let world_bytes = serialize_world(&sim.world)?;

    // Audit 3 (Phase 2): encode the transient caches with entity ordinals so
    // they survive the entity renumbering that column deserialization does.
    let mut agent_ord: FxHashMap<hecs::Entity, usize> = FxHashMap::default();
    for (i, (e, _)) in sim
        .world
        .query::<(&AgentUid, &Metabolism)>()
        .iter()
        .enumerate()
    {
        agent_ord.insert(e, i);
    }
    let mut grain_ord: FxHashMap<hecs::Entity, usize> = FxHashMap::default();
    for (i, (e, _)) in sim.world.query::<&Grain>().iter().enumerate() {
        grain_ord.insert(e, i);
    }
    let neighbor_cache = sim
        .neighbor_cache
        .iter()
        .map(|(e, c)| NeighborCacheSer {
            agent: agent_ord.get(e).copied().unwrap_or(usize::MAX),
            neighbors: c
                .neighbors
                .iter()
                .map(|n| agent_ord.get(n).copied().unwrap_or(usize::MAX))
                .collect(),
            last_count: c.last_count,
            last_vel: [c.last_vel.x, c.last_vel.y],
        })
        .collect();
    let grain_vis_cache = sim
        .grain_vis_cache
        .iter()
        .map(|(e, c)| GrainVisCacheSer {
            agent: agent_ord.get(e).copied().unwrap_or(usize::MAX),
            pos: [c.pos.x, c.pos.y],
            heading: c.heading,
            vision_range: c.vision_range,
            grains_version: c.grains_version,
            visible: c
                .visible
                .iter()
                .map(|(ge, p, amt)| {
                    (
                        grain_ord.get(ge).copied().unwrap_or(usize::MAX),
                        [p.x, p.y],
                        *amt,
                    )
                })
                .collect(),
        })
        .collect();

    Ok(Checkpoint {
        version: CHECKPOINT_VERSION,
        config: sim.config.clone(),
        rng: sim.rng.clone(),
        time: sim.time,
        environment: sim.environment,
        physics: sim.physics.to_state(),
        session_id: sim.session_id,
        next_uid: sim.next_uid,
        deaths: sim.deaths,
        predator_kills: sim.predator_kills,
        grains_consumed: sim.grains_consumed,
        death_ages: sim.death_ages.clone(),
        events_log: sim.events_log.clone(),
        total_energy_intake_kj: sim.total_energy_intake_kj,
        total_energy_expenditure_kj: sim.total_energy_expenditure_kj,
        total_energy_lost_at_death_kj: sim.total_energy_lost_at_death_kj,
        total_energy_inflow_spawn_kj: sim.total_energy_inflow_spawn_kj,
        obstacles: sim.obstacles.clone(),
        world_bytes,
        grains_version: sim.grains_version,
        neighbor_cache,
        grain_vis_cache,
    })
}
