use rapier2d::prelude::*;
use nalgebra::Vector2;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Sprint 2 (Audit 5, B9/B24): coarse broad-phase over STATIC obstacles.
///
/// Static geometry (walls + boxes) is registered here when `add_wall` /
/// `add_obstacle` run. `may_hit` answers "could this segment touch ANY static
/// collider?" with a cheap AABB overlap test over a uniform cell grid — it
/// never touches Rapier. Only when it returns `true` does `cast_ray_to_static`
/// issue the authoritative `QueryPipeline::cast_ray`. Because static geometry
/// is immutable, the grid is built once at `Simulation::new` and stays valid.
///
/// Grid cells store the AABB *min* corner of each obstacle (indexed by the
/// cell that contains it); queries scan the cell range the segment's bounding
/// box covers and test the true AABB. This is conservative (may report a
/// possible hit that misses) but never misses a true hit, so determinism is
/// preserved.
#[derive(Default, Clone)]
pub struct StaticObstacleBroadphase {
    /// (min, max) of each registered static AABB.
    aabbs: Vec<(Vector2<f64>, Vector2<f64>)>,
    /// cell key (reusing the spatial hash packing) → obstacle indices in `aabbs`.
    cells: HashMap<u64, Vec<usize>>,
    cell_size: f64,
}

impl StaticObstacleBroadphase {
    pub fn new(cell_size: f64) -> Self {
        Self {
            aabbs: Vec::new(),
            cells: HashMap::new(),
            cell_size,
        }
    }

    fn cell_of(&self, x: f64, y: f64) -> (i64, i64) {
        let cx = (x / self.cell_size).floor() as i64;
        let cy = (y / self.cell_size).floor() as i64;
        let ox = (cx as i64).wrapping_add(i32::MAX as i64);
        let oy = (cy as i64).wrapping_add(i32::MAX as i64);
        (ox, oy)
    }

    fn key_of(cx: i64, cy: i64) -> u64 {
        ((cx as u64) << 32) | (cy as u64 & 0xFFFFFFFF)
    }

    pub fn insert(&mut self, min: Vector2<f64>, max: Vector2<f64>) {
        let idx = self.aabbs.len();
        self.aabbs.push((min, max));
        // Register into EVERY cell the AABB overlaps, so any ray whose cell
        // range intersects the box is guaranteed to probe it (a center-cell-only
        // index could miss a large obstacle a straight ray passes through).
        let (min_cx, min_cy) = self.cell_of(min.x, min.y);
        let (max_cx, max_cy) = self.cell_of(max.x, max.y);
        for cx in min_cx..=max_cx {
            for cy in min_cy..=max_cy {
                self.cells.entry(Self::key_of(cx, cy)).or_default().push(idx);
            }
        }
    }

    /// Segment-vs-AABB overlap test (slab method), t ∈ [0, max_toi].
    fn segment_hits_aabb(
        origin: Vector2<f64>,
        dir: Vector2<f64>,
        max_toi: f64,
        min: Vector2<f64>,
        max: Vector2<f64>,
    ) -> bool {
        // Pad by a small fraction so a ray that f64 math deems a clean miss but
        // that f32 Rapier (the authoritative query) would barely graze is still
        // forwarded — the broad-phase may over-report, never under-report.
        const PAD: f64 = 1e-3;
        let min = min - Vector2::new(PAD, PAD);
        let max = max + Vector2::new(PAD, PAD);
        let mut t0 = 0.0f64;
        let mut t1 = max_toi;
        let axes: [(f64, f64, f64, f64); 2] = [
            (origin.x, dir.x, min.x, max.x),
            (origin.y, dir.y, min.y, max.y),
        ];
        for (oc, dc, lo, hi) in axes {
            if dc.abs() < 1e-12 {
                if oc < lo - 1e-9 || oc > hi + 1e-9 {
                    return false;
                }
            } else {
                let mut a = (lo - oc) / dc;
                let mut b = (hi - oc) / dc;
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                t0 = t0.max(a);
                t1 = t1.min(b);
                if t0 > t1 {
                    return false;
                }
            }
        }
        t0 <= t1
    }

    /// Conservative: returns true when the segment COULD intersect a registered
    /// static AABB. Never returns false for a ray that would hit — so falling
    /// back to the authoritative Rapier cast on `true` preserves determinism.
    pub fn may_hit(&self, origin: Vector2<f64>, dir: Vector2<f64>, max_toi: f64) -> bool {
        if self.aabbs.is_empty() {
            return false;
        }
        let end = origin + dir * max_toi;
        let min_cell = self.cell_of(origin.x.min(end.x), origin.y.min(end.y));
        let max_cell = self.cell_of(origin.x.max(end.x), origin.y.max(end.y));
        for cx in min_cell.0..=max_cell.0 {
            for cy in min_cell.1..=max_cell.1 {
                if let Some(indices) = self.cells.get(&Self::key_of(cx, cy)) {
                    for &i in indices {
                        let (min, max) = self.aabbs[i];
                        if Self::segment_hits_aabb(origin, dir, max_toi, min, max) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.aabbs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.aabbs.is_empty()
    }

    /// Registered (min, max) pairs, in insertion order. Used to carry the
    /// broad-phase across a checkpoint roundtrip (`PhysicsState`).
    pub fn aabbs(&self) -> &[(Vector2<f64>, Vector2<f64>)] {
        &self.aabbs
    }
}

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
    /// Sprint 2 (Audit 5, B9): static AABBs must survive a checkpoint roundtrip
    /// so a restored world keeps its LOS broad-phase (it holds no Rapier state).
    pub static_aabbs: Vec<(Vector2<f64>, Vector2<f64>)>,
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
    /// Sprint 2 (Audit 5, B9/B24): coarse static-obstacle broad-phase. A cheap
    /// segment-vs-AABB test over static geometry runs BEFORE any Rapier
    /// raycast; only rays that could touch a static collider fall through to
    /// the authoritative `QueryPipeline::cast_ray`. Keeps the urban-LOS raycast
    /// count from growing multiplicatively with agents × neighbors × grains —
    /// open-space rays never reach Rapier at all.
    pub static_broadphase: StaticObstacleBroadphase,
    /// Sprint 2 (Audit 5, B9/B24): the number of AUTHORITATIVE Rapier raycasts
    /// actually issued (`los_raycast_count` excludes coarse-cleared rays).
    /// Resettable so tests/telemetry can measure raycast traffic per frame.
    /// `AtomicU64` so `cast_ray_to_static` (a `&self` method, called from the
    /// simulation hot loop under an immutable borrow of `physics`) can tally.
    pub los_raycast_count: std::sync::atomic::AtomicU64,
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
            static_broadphase: StaticObstacleBroadphase::new(2.0),
            los_raycast_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    pub fn add_wall(&mut self, p1: Vector2<f32>, p2: Vector2<f32>) {
        let collider = ColliderBuilder::segment(nalgebra::Point2::new(p1.x, p1.y), nalgebra::Point2::new(p2.x, p2.y)).build();
        self.colliders.insert(collider);
        // Static geometry must be queryable immediately (the query pipeline's
        // BVH is otherwise only refreshed inside `step`).
        self.query_pipeline.update(&self.bodies, &self.colliders);
        // Sprint 2 (Audit 5, B9): register the wall's AABB in the coarse
        // broad-phase so LOS raycasts cull before touching Rapier.
        self.static_broadphase.insert(
            Vector2::new(p1.x.min(p2.x) as f64, p1.y.min(p2.y) as f64),
            Vector2::new(p1.x.max(p2.x) as f64, p1.y.max(p2.y) as f64),
        );
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
        // Sprint 2 (Audit 5, B9): register the box in the coarse broad-phase.
        self.static_broadphase.insert(min, max);
    }

    /// 4.3: line-of-sight query — does any STATIC collider (wall + obstacle)
    /// intersect the segment `origin → origin + dir` before `max_toi` (in
    /// `dir` units)? Dynamic bodies (agents, predators) never block vision.
    /// Returns the time-of-impact of the first static hit, if any.
    ///
    /// Sprint 2 (Audit 5, B9/B24): a coarse segment-vs-AABB test over the
    /// static broad-phase runs FIRST. Only rays that could touch a static
    /// collider are forwarded to the authoritative Rapier raycast (counted in
    /// `los_raycast_count`); open-space rays short-circuit to `None` without
    /// touching the query pipeline. Determinism is preserved because `may_hit`
    /// is conservative — it never reports "clear" for a ray that would hit.
    pub fn cast_ray_to_static(&self, origin: Vector2<f64>, dir: Vector2<f64>, max_toi: f64) -> Option<f64> {
        if !self.static_broadphase.may_hit(origin, dir, max_toi) {
            return None;
        }
        self.los_raycast_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ray = Ray::new(
            nalgebra::Point2::new(origin.x as f32, origin.y as f32),
            Vector2::new(dir.x as f32, dir.y as f32),
        );
        self.query_pipeline
            .cast_ray(&self.bodies, &self.colliders, &ray, max_toi as f32, true, QueryFilter::only_fixed())
            .map(|(_, toi)| toi as f64)
    }

    /// Sprint 2 (Audit 5, B9): reset the authoritative-raycast counter so a
    /// caller can measure raycast traffic per frame.
    pub fn reset_raycast_count(&mut self) {
        self.los_raycast_count.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Sprint 2 (Audit 5, B9): number of authoritative Rapier raycasts issued
    /// since the last reset.
    pub fn los_raycast_count(&self) -> u64 {
        self.los_raycast_count.load(std::sync::atomic::Ordering::Relaxed)
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

    /// Sprint 2 (Audit 5, B10): set a dynamic body's velocity WITHOUT forcing a
    /// wake-up unless the velocity actually changed materially or the body is
    /// transitioning to an active state. The old hot loop called
    /// `set_linvel(v, true)` for every agent every tick — the `true` flag wakes
    /// the body, so roosting/idle (or actually sleeping) bodies were force-woken
    /// 60×/s, growing the active island set and solver cost. Returns true if the
    /// body was touched (velocity written).
    ///
    /// Rules:
    /// - a body that is asleep and requested to stay at (near) rest is left
    ///   untouched (no wake-up, no write);
    /// - a body whose velocity did not materially change is left untouched;
    /// - a genuinely different velocity is written with `wake_up = true`.
    pub fn set_linvel_if_changed(&mut self, handle: u64, v: Vector2<f32>, eps: f32) -> bool {
        let Some(rb) = self.bodies.get_mut(unpack_handle(handle)) else {
            return false;
        };
        let current = rb.linvel();
        let sleeping = rb.is_sleeping();
        // Sleeping at rest: leave it asleep.
        if sleeping && v.norm() <= eps {
            return false;
        }
        // Awake (or moving to rest): skip only when genuinely unchanged.
        if (current - v).norm_squared() <= eps * eps {
            return false;
        }
        rb.set_linvel(v, true);
        true
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

    /// Sprint 1 (Audit 5): set the fixed simulation timestep actually used by
    /// Rapier's `IntegrationParameters`. The default `PhysicsWorld::new()` uses
    /// 1/120; `Simulation::new` drives this from `SimulationConfig::dt` so the
    /// ECS clock and the physics solver never drift apart.
    pub fn set_dt(&mut self, dt: f64) {
        self.integration_parameters.dt = dt as f32;
    }

    /// Sprint 1 (Audit 5): set the global gravity vector. `Simulation::new`
    /// passes `SimulationConfig::gravity` here so a scenario can configure it;
    /// the default (0, 0) keeps the current top-down arena behavior.
    pub fn set_gravity(&mut self, gravity: Vector2<f32>) {
        self.gravity = gravity;
    }

    /// Sprint 1 (Audit 5): current fixed timestep used by Rapier.
    pub fn dt(&self) -> f64 {
        self.integration_parameters.dt as f64
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
            static_aabbs: self.static_broadphase.aabbs().to_vec(),
        }
    }

    /// 3.6: restore a checkpointed physics state (fresh pipeline, same data).
    pub fn from_state(state: PhysicsState) -> Self {
        let mut static_broadphase = StaticObstacleBroadphase::new(2.0);
        for &(min, max) in &state.static_aabbs {
            static_broadphase.insert(min, max);
        }
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
            static_broadphase,
            los_raycast_count: std::sync::atomic::AtomicU64::new(0),
        }
    }
}