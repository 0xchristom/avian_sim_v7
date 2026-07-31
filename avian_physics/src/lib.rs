use rapier2d::prelude::*;
use nalgebra::Vector2;

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

    pub fn step(&mut self) {
        let physics_hooks = ();
        let event_handler = ();
        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline), // Dodano Some()
            &physics_hooks,
            &event_handler,
        );
    }

    pub fn init_ellipse_collider(half_extents: Vector2<f32>) -> Collider {
        ColliderBuilder::ball(half_extents.x.max(half_extents.y)).build()
    }

    pub fn init_head_collider(_offset: Vector2<f32>, radius: f32) -> Collider {
        ColliderBuilder::ball(radius).build()
    }

    pub fn rigid_body_config(mass: f32, half_extents: Vector2<f32>) -> (RigidBody, Collider) {
        let inertia = (2.0 / 5.0) * mass * (half_extents.x.powi(2) + half_extents.y.powi(2));
        let rb = RigidBodyBuilder::dynamic()
            .additional_mass_properties(MassProperties::new(nalgebra::Point2::new(0.0, 0.0), mass, inertia))
            .build();
        let collider = ColliderBuilder::ball(half_extents.x.max(half_extents.y))
            .restitution(0.2)
            .friction(0.8)
            .build();
        (rb, collider)
    }
}