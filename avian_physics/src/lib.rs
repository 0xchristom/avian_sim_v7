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

    pub fn add_wall(&mut self, p1: Vector2<f32>, p2: Vector2<f32>) {
        let collider = ColliderBuilder::segment(nalgebra::Point2::new(p1.x, p1.y), nalgebra::Point2::new(p2.x, p2.y))
            .build();
        self.colliders.insert(collider);
    }

    pub fn spawn_agent_body(&mut self, pos: Vector2<f32>, mass: f32) -> RigidBodyHandle {
        let rb = RigidBodyBuilder::dynamic()
            .translation(nalgebra::Vector2::new(pos.x, pos.y))
            .additional_mass_properties(MassProperties::new(nalgebra::Point2::new(0.0, 0.0), mass, 0.01))
            .linear_damping(1.0)
            .angular_damping(1.0)
            .build();
        
        // Najpierw wstawiamy ciało sztywne do zestawu ciał
        let handle = self.bodies.insert(rb);
        
        // Następnie tworzymy kolidery i przypisujemy je do ciała za pomocą uchwytu
        let collider = ColliderBuilder::ball(0.4)
            .restitution(0.2)
            .friction(0.8)
            .build();
        self.colliders.insert_with_parent(collider, handle, &mut self.bodies);
        
        handle
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
            Some(&mut self.query_pipeline),
            &physics_hooks,
            &event_handler,
        );
    }
}