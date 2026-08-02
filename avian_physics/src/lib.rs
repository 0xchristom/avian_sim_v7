use rapier2d::prelude::*;
use nalgebra::Vector2;
use serde::{Serialize, Deserialize};

/// 3.6 checkpoint state: the entire physics world EXCEPT `PhysicsPipeline`,
/// which rapier deliberately keeps unserializable (workspace-only scratch
/// buffers). Snapshotting the state and rebuilding a fresh pipeline on load
/// is byte-for-byte equivalent — the pipeline holds no persistent simulation
/// data.
#[derive(Serialize, Deserialize)]
pub struct PhysicsState {
    pub gravity: Vector2<f32>,
    pub integration_parameters: IntegrationParameters,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhase,
    pub narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,
}

/// Pack a `RigidBodyHandle` (index, generation) into a neutral `u64`.
///
/// Rapier reuses freed body slots with a NEW generation, so storing only the
/// index (as the old code did) makes `get`/`get_mut` return `None` for any
/// body spawned after a despawn — the body stays frozen in place. The
/// generation half must be preserved (fix for the multi-predator "stuck"
/// report and for immigration-spawned agents freezing in long runs).
fn pack_handle(h: RigidBodyHandle) -> u64 {
    let (index, generation) = h.into_raw_parts();
    ((generation as u64) << 32) | (index as u64)
}

fn unpack_handle(raw: u64) -> RigidBodyHandle {
    RigidBodyHandle::from_raw_parts((raw & 0xFFFF_FFFF) as u32, (raw >> 32) as u32)
}

pub struct PhysicsWorld {
    pub pipeline: PhysicsPipeline,
    pub gravity: Vector2<f32>,
    pub integration_parameters: IntegrationParameters,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhase,
    pub narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    pub query_pipeline: QueryPipeline,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        let integration_parameters = IntegrationParameters {
            dt: 1.0 / 120.0,
            num_solver_iterations: std::num::NonZero::new(16).unwrap(),
            ..Default::default()
        };
        Self {
            pipeline: PhysicsPipeline::new(),
            gravity: Vector2::zeros(),
            integration_parameters,
            island_manager: IslandManager::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
        }
    }

    pub fn add_wall(&mut self, p1: Vector2<f32>, p2: Vector2<f32>) {
        let collider = ColliderBuilder::segment(nalgebra::Point2::new(p1.x, p1.y), nalgebra::Point2::new(p2.x, p2.y)).build();
        self.colliders.insert(collider);
        // Static geometry must be queryable immediately (the query pipeline's
        // BVH is otherwise only refreshed inside `step`).
        self.query_pipeline.update(&self.bodies, &self.colliders);
    }

    /// 4.3: a static box obstacle (building, water, tree) spanning `min..=max`
    /// in world meters. Attached to an explicit fixed body so it is both a
    /// physics barrier (dynamic bodies bounce off) and part of the static set
    /// used by line-of-sight raycasts (`QueryFilter::only_fixed`).
    pub fn add_obstacle(&mut self, min: Vector2<f64>, max: Vector2<f64>) {
        let center = Vector2::new((min.x + max.x) as f32 / 2.0, (min.y + max.y) as f32 / 2.0);
        let half = Vector2::new((max.x - min.x) as f32 / 2.0, (max.y - min.y) as f32 / 2.0);
        let rb = RigidBodyBuilder::fixed().translation(center).build();
        let handle = self.bodies.insert(rb);
        let collider = ColliderBuilder::cuboid(half.x, half.y).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        self.query_pipeline.update(&self.bodies, &self.colliders);
    }

    /// 4.3: line-of-sight query — does any STATIC collider (wall + obstacle)
    /// intersect the segment `origin → origin + dir` before `max_toi` (in
    /// `dir` units)? Dynamic bodies (agents, predators) never block vision.
    /// Returns the time-of-impact of the first static hit, if any.
    pub fn cast_ray_to_static(&self, origin: Vector2<f64>, dir: Vector2<f64>, max_toi: f64) -> Option<f64> {
        let ray = Ray::new(
            nalgebra::Point2::new(origin.x as f32, origin.y as f32),
            Vector2::new(dir.x as f32, dir.y as f32),
        );
        self.query_pipeline
            .cast_ray(&self.bodies, &self.colliders, &ray, max_toi as f32, true, QueryFilter::only_fixed())
            .map(|(_, toi)| toi as f64)
    }

    pub fn spawn_agent_body(&mut self, pos: Vector2<f32>, mass: f32) -> u64 {
        let rb = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector2::new(pos.x, pos.y))
            .additional_mass_properties(MassProperties::new(nalgebra::Point2::new(0.0, 0.0), mass, 0.01))
            .linear_damping(1.0)
            .angular_damping(1.0) // Zapobiega niekontrolowanemu wirowaniu kuli
            .build();
        
        let handle = self.bodies.insert(rb);
        let collider = ColliderBuilder::ball(0.4).restitution(0.2).friction(0.8).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);

        pack_handle(handle)
    }

    /// 2.2: predator body — larger ball so it reads as a bulkier threat.
    pub fn spawn_predator_body(&mut self, pos: Vector2<f32>, mass: f32) -> u64 {
        let rb = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector2::new(pos.x, pos.y))
            .additional_mass_properties(MassProperties::new(nalgebra::Point2::new(0.0, 0.0), mass, 0.05))
            .linear_damping(1.0)
            .angular_damping(1.0)
            .build();
        let handle = self.bodies.insert(rb);
        let collider = ColliderBuilder::ball(0.5).restitution(0.2).friction(0.8).build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        pack_handle(handle)
    }

    /// 2.4: remove a body (and its attached colliders) when an agent/predator dies.
    pub fn remove_body(&mut self, handle: u64) {
        let h = unpack_handle(handle);
        self.bodies.remove(
            h,
            &mut self.island_manager,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    pub fn get_body(&self, handle: u64) -> Option<&RigidBody> {
        self.bodies.get(unpack_handle(handle))
    }

    pub fn get_body_mut(&mut self, handle: u64) -> Option<&mut RigidBody> {
        self.bodies.get_mut(unpack_handle(handle))
    }

    pub fn step(&mut self) {
        let physics_hooks = ();
        let event_handler = ();
        self.pipeline.step(
            &self.gravity, &self.integration_parameters, &mut self.island_manager, &mut self.broad_phase,
            &mut self.narrow_phase, &mut self.bodies, &mut self.colliders, &mut self.impulse_joints,
            &mut self.multibody_joints, &mut self.ccd_solver, Some(&mut self.query_pipeline),
            &physics_hooks, &event_handler,
        );
    }

    /// 3.6: snapshot all persistent physics state (pipeline excluded — see
    /// `PhysicsState`).
    pub fn to_state(&self) -> PhysicsState {
        PhysicsState {
            gravity: self.gravity,
            integration_parameters: self.integration_parameters,
            island_manager: self.island_manager.clone(),
            broad_phase: self.broad_phase.clone(),
            narrow_phase: self.narrow_phase.clone(),
            bodies: self.bodies.clone(),
            colliders: self.colliders.clone(),
            impulse_joints: self.impulse_joints.clone(),
            multibody_joints: self.multibody_joints.clone(),
            ccd_solver: self.ccd_solver.clone(),
            query_pipeline: self.query_pipeline.clone(),
        }
    }

    /// 3.6: restore a checkpointed physics state (fresh pipeline, same data).
    pub fn from_state(state: PhysicsState) -> Self {
        Self {
            pipeline: PhysicsPipeline::new(),
            gravity: state.gravity,
            integration_parameters: state.integration_parameters,
            island_manager: state.island_manager,
            broad_phase: state.broad_phase,
            narrow_phase: state.narrow_phase,
            bodies: state.bodies,
            colliders: state.colliders,
            impulse_joints: state.impulse_joints,
            multibody_joints: state.multibody_joints,
            ccd_solver: state.ccd_solver,
            query_pipeline: state.query_pipeline,
        }
    }
}