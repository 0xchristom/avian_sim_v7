use avian_core::components::*;
use avian_core::rng::SimRng;
use nalgebra::Vector2;
use hecs::Entity;
use rand_distr::Distribution;

pub struct AgentContext<'a> {
    pub pos: &'a mut Position,
    pub head: &'a mut Heading,
    pub vel: &'a mut Velocity,
    pub meta: &'a mut Metabolism,
    pub fsm: &'a mut FSMState,
    pub levy: &'a mut LevyState,
    pub mass: &'a Mass,
    pub vision: &'a Vision,
    pub head_bob: &'a mut HeadBob,
    pub neighbors: Vec<(Entity, f64)>,
    pub grains: Vec<(Entity, Vector2<f64>, u32)>,
    pub rng: &'a mut SimRng,
}

pub enum BTStatus {
    Success,
    Failure,
    Running,
}

pub trait BTNode {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus;
}

pub struct Sequence {
    children: Vec<Box<dyn BTNode>>,
}

impl BTNode for Sequence {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus {
        for child in &self.children {
            match child.tick(ctx) {
                BTStatus::Success => continue,
                BTStatus::Failure => return BTStatus::Failure,
                BTStatus::Running => return BTStatus::Running,
            }
        }
        BTStatus::Success
    }
}

pub struct Selector {
    children: Vec<Box<dyn BTNode>>,
}

impl BTNode for Selector {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus {
        for child in &self.children {
            match child.tick(ctx) {
                BTStatus::Success => return BTStatus::Success,
                BTStatus::Failure => continue,
                BTStatus::Running => return BTStatus::Running,
            }
        }
        BTStatus::Failure
    }
}

pub struct Action(pub fn(&mut AgentContext) -> BTStatus);

impl BTNode for Action {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus {
        (self.0)(ctx)
    }
}

// --- Akcje BT ---

fn foraging_action(ctx: &mut AgentContext) -> BTStatus {
    if ctx.meta.hunger < 0.4 || ctx.grains.is_empty() {
        return BTStatus::Failure;
    }

    *ctx.fsm = FSMState::Foraging;
    let (_g_id, g_pos, _) = ctx.grains[0]; // Najbliższe ziarno
    let dist = (g_pos - ctx.pos.0).norm();

    if dist < 0.5 {
        ctx.vel.0 = Vector2::zeros();
        BTStatus::Running
    } else {
        let dir = (g_pos - ctx.pos.0).normalize();
        ctx.head.0 = dir.y.atan2(dir.x);
        ctx.vel.0 = Vector2::new(1.0 * ctx.head.0.cos(), 1.0 * ctx.head.0.sin());
        BTStatus::Running
    }
}

fn spacer_action(ctx: &mut AgentContext) -> BTStatus {
    *ctx.fsm = FSMState::Spacer;
    let speed = 1.0;
    
    if ctx.levy.remaining_dist <= 0.0 {
        let u: f64 = ctx.rng.gen();
        let dist = 1.0 * (1.0 - u).powf(-1.0 / 2.0).min(5.0);
        let normal = rand_distr::Normal::new(0.0f64, 0.5f64).unwrap();
        ctx.levy.target_heading = ctx.head.0 + normal.sample(ctx.rng);
        ctx.levy.remaining_dist = dist;
    } else {
        ctx.levy.remaining_dist -= speed * 0.00833; // dt approx
    }
    
    let mut diff = ctx.levy.target_heading - ctx.head.0;
    while diff > std::f64::consts::PI { diff -= std::f64::consts::TAU; }
    while diff < -std::f64::consts::PI { diff += std::f64::consts::TAU; }
    ctx.head.0 += diff.clamp(-0.016, 0.016);
    
    ctx.vel.0 = Vector2::new(speed * ctx.head.0.cos(), speed * ctx.head.0.sin());
    BTStatus::Running
}

pub fn build_default_tree() -> Box<dyn BTNode> {
    Box::new(Selector {
        children: vec![
            Box::new(Action(foraging_action)),
            Box::new(Action(spacer_action)),
        ],
    })
}