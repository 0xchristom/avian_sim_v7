pub struct RLObservation {
    pub vector: [f32; 128],
}

pub struct RLAction {
    pub discrete: Option<DiscreteAction>,
    pub continuous: Option<(f32, f32)>,
}

pub enum DiscreteAction {
    Idle,
    Walk,
    Run,
    Peck,
    ScanLeft,
    ScanRight,
    Flee,
}

pub struct RLReward {
    pub survival: f32,
    pub energy_efficiency: f32,
    pub flock_cohesion: f32,
    pub predator_avoidance: f32,
    pub total: f32,
}

impl RLReward {
    pub fn compute(energy_kj: f32, max_energy: f32, dist_to_flock: f32, alarm_triggered: bool) -> Self {
        let survival = 1.0;
        let energy_efficiency = energy_kj / max_energy;
        let flock_cohesion = 1.0 / (1.0 + dist_to_flock);
        let predator_avoidance = if alarm_triggered { -10.0 } else { 0.0 };
        
        let total = 0.4 * survival + 0.3 * energy_efficiency + 0.2 * flock_cohesion + 0.1 * predator_avoidance;
        
        Self {
            survival,
            energy_efficiency,
            flock_cohesion,
            predator_avoidance,
            total,
        }
    }
}

pub fn state_to_observation(agent: &avian_core::AgentSnapshot) -> RLObservation {
    let mut vec = [0.0f32; 128];
    vec[0] = agent.pos[0] as f32;
    vec[1] = agent.pos[1] as f32;
    vec[2] = agent.energy_kj as f32;
    vec[3] = agent.hunger as f32;
    RLObservation { vector: vec }
}