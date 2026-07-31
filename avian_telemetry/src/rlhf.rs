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

pub fn state_to_observation(agent_pos: [f32; 2], agent_energy: f32, agent_hunger: f32, neighbors: &[[f32; 2]], grains: &[[f32; 2]]) -> RLObservation {
    let mut vec = [0.0f32; 128];
    
    vec[0] = agent_pos[0];
    vec[1] = agent_pos[1];
    vec[2] = agent_energy;
    vec[3] = agent_hunger;
    
    for (i, n) in neighbors.iter().take(7).enumerate() {
        let base = 16 + i * 7;
        vec[base] = n[0] - agent_pos[0];
        vec[base + 1] = n[1] - agent_pos[1];
    }
    
    for (i, g) in grains.iter().take(5).enumerate() {
        let base = 65 + i * 3;
        vec[base] = g[0] - agent_pos[0];
        vec[base + 1] = g[1] - agent_pos[1];
    }
    
    RLObservation { vector: vec }
}
