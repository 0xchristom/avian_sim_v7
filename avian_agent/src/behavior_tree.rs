use avian_core::rng::SimRng;

pub struct AgentContext<'a> {
    pub entity: hecs::Entity,
    pub world: &'a mut hecs::World,
    pub rng: &'a mut SimRng,
    pub attention_budget: AttentionBudget,
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

pub struct AttentionBudget {
    pub total: f64,
    pub allocated: f64,
}

impl AttentionBudget {
    pub fn allocate(&mut self, amount: f64) -> bool {
        if self.allocated + amount <= self.total {
            self.allocated += amount;
            true
        } else {
            false
        }
    }
    
    pub fn release(&mut self, amount: f64) {
        self.allocated -= amount.min(self.allocated);
    }
}

pub fn idle_action(ctx: &mut AgentContext) -> BTStatus {
    if ctx.attention_budget.allocate(0.1) {
        BTStatus::Running
    } else {
        BTStatus::Failure
    }
}

pub fn foraging_action(ctx: &mut AgentContext) -> BTStatus {
    if ctx.attention_budget.allocate(0.7) {
        BTStatus::Running
    } else {
        BTStatus::Failure
    }
}

pub fn scan_action(ctx: &mut AgentContext) -> BTStatus {
    if ctx.attention_budget.allocate(0.3) {
        BTStatus::Running
    } else {
        BTStatus::Failure
    }
}
