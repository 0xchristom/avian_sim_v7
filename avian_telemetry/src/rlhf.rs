use avian_core::AgentSnapshot;
use avian_core::calibration;

pub struct RLObservation { pub vector: [f32; 128] }

pub struct RLAction { pub discrete: Option<DiscreteAction>, pub continuous: Option<(f32, f32)> }

pub enum DiscreteAction { Idle, Walk, Run, Peck, ScanLeft, ScanRight, Flee }

/// 3.2 event-driven reward with a per-component breakdown for telemetry
/// debugging. Per-second rates (flocking, starvation) are pre-multiplied by
/// `dt` at the call site so they balance against the one-shot rewards.
pub struct RLReward {
    /// +1.0 per grain consumed (one-shot).
    pub grain: f32,
    /// +0.1/sec within 2 m of ≥ 2 agents.
    pub flocking: f32,
    /// -0.01/sec while energy < 20% of max.
    pub starvation: f32,
    /// -10.0 captured by a predator (one-shot).
    pub captured: f32,
    /// +0.5 a fleeing episode ends without capture (one-shot).
    pub flee_success: f32,
    pub total: f32,
}

impl RLReward {
    pub fn compute(
        dt: f32,
        energy_kj: f32,
        max_energy: f32,
        flock_neighbors: usize,
        alarm_triggered: bool,
        was_alarmed: bool,
        grain_eaten: bool,
        captured: bool,
    ) -> Self {
        let grain = if grain_eaten { calibration::REWARD_GRAIN as f32 } else { 0.0 };
        let flocking = if flock_neighbors >= calibration::REWARD_FLOCK_NEIGHBORS_MIN {
            calibration::REWARD_FLOCKING_PER_S as f32 * dt
        } else {
            0.0
        };
        let starvation = if energy_kj < calibration::REWARD_STARVATION_ENERGY_FRACTION as f32 * max_energy {
            -calibration::REWARD_STARVATION_PER_S as f32 * dt
        } else {
            0.0
        };
        let captured = if captured { calibration::REWARD_CAPTURED as f32 } else { 0.0 };
        let flee_success = if was_alarmed && !alarm_triggered && captured == 0.0 {
            calibration::REWARD_FLEE_SUCCESS as f32
        } else {
            0.0
        };
        let total = grain + flocking + starvation + captured + flee_success;
        Self { grain, flocking, starvation, captured, flee_success, total }
    }
}

/// 3.1 `obs_v1` — 128 dims, frozen layout (extension is a versioned `obs_v2`,
/// never an in-place edit; `metadata.json` is the schema authority, 3.7).
///
/// ```text
///   [0..2]   pos (normalized to world size)
///   [2..4]   heading (sin + cos)
///   [4]      velocity magnitude
///   [5]      energy (normalized)
///   [6]      hunger
///   [7]      age (normalized to WILD_MAX_LIFESPAN_YEARS)
///   [8]      vitality
///   [9]      light_level
///   [10..16] nearest 3 grains rel pos (2 each)
///   [16..37] 7 neighbors rel pos + dist (3 each)
///   [37..43] predator rel pos + threat + alarm_flag (+ 2 reserved)
///   [43..51] memory locations (4.2 — all zero until it ships)
///   [51..127] reserved — all zero (future fields without renumbering)
///   [127]    unused (kept zero)
/// ```
pub fn state_to_observation(
    agent: &AgentSnapshot,
    neighbors: &[[f64; 2]],
    grains: &[[f64; 2]],
    predators: &[[f64; 2]],
    light_level: f64,
) -> RLObservation {
    let mut vec = [0.0f32; 128];

    // Normalized ego state.
    vec[0] = (agent.pos[0] / calibration::WORLD_WIDTH_M) as f32;
    vec[1] = (agent.pos[1] / calibration::WORLD_HEIGHT_M) as f32;
    vec[2] = agent.heading.cos() as f32;
    vec[3] = agent.heading.sin() as f32;
    vec[4] = (agent.vel[0].powi(2) + agent.vel[1].powi(2)).sqrt() as f32;
    vec[5] = (agent.energy_kj / calibration::MAX_ENERGY_KJ).clamp(0.0, 1.0) as f32;
    vec[6] = agent.hunger as f32;
    vec[7] = (agent.age_years / calibration::WILD_MAX_LIFESPAN_YEARS).clamp(0.0, 1.0) as f32;
    vec[8] = agent.vitality as f32;
    vec[9] = light_level as f32;

    // Nearest OBS_GRAIN_COUNT grains, relative positions.
    let mut grains_sorted: Vec<&[f64; 2]> = grains.iter().collect();
    grains_sorted.sort_by(|a, b| {
        let da = ((a[0] - agent.pos[0]).powi(2) + (a[1] - agent.pos[1]).powi(2)).sqrt();
        let db = ((b[0] - agent.pos[0]).powi(2) + (b[1] - agent.pos[1]).powi(2)).sqrt();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, g) in grains_sorted.iter().take(calibration::OBS_GRAIN_COUNT).enumerate() {
        let base = 10 + i * 2;
        vec[base] = (g[0] - agent.pos[0]) as f32;
        vec[base + 1] = (g[1] - agent.pos[1]) as f32;
    }

    // OBS_NEIGHBOR_COUNT nearest neighbors, relative pos + distance.
    let mut neigh_sorted: Vec<&[f64; 2]> = neighbors.iter().collect();
    neigh_sorted.sort_by(|a, b| {
        let da = ((a[0] - agent.pos[0]).powi(2) + (a[1] - agent.pos[1]).powi(2)).sqrt();
        let db = ((b[0] - agent.pos[0]).powi(2) + (b[1] - agent.pos[1]).powi(2)).sqrt();
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, n) in neigh_sorted.iter().take(calibration::OBS_NEIGHBOR_COUNT).enumerate() {
        let base = 16 + i * 3;
        vec[base] = (n[0] - agent.pos[0]) as f32;
        vec[base + 1] = (n[1] - agent.pos[1]) as f32;
        vec[base + 2] = ((n[0] - agent.pos[0]).powi(2) + (n[1] - agent.pos[1]).powi(2)).sqrt() as f32;
    }

    // Predator block: nearest predator rel pos + threat magnitude + alarm flag.
    let mut threat_mag = 0.0f32;
    let mut nearest_pred: Option<(f64, f64)> = None; // (dx, dy)
    for p in predators {
        let dx = p[0] - agent.pos[0];
        let dy = p[1] - agent.pos[1];
        let d = (dx * dx + dy * dy).sqrt();
        if nearest_pred.map_or(true, |(_, _)| true) {
            // Keep the closest.
            let keep = nearest_pred
                .map(|(dx0, dy0)| {
                    let d0 = (dx0 * dx0 + dy0 * dy0).sqrt();
                    d < d0
                })
                .unwrap_or(true);
            if keep {
                nearest_pred = Some((dx, dy));
                threat_mag = (1.0 / (1.0 + d)) as f32;
            }
        }
    }
    if let Some((dx, dy)) = nearest_pred {
        vec[37] = dx as f32;
        vec[38] = dy as f32;
    }
    vec[39] = threat_mag;
    vec[40] = if agent.alarm_triggered { 1.0 } else { 0.0 };

    // [43..51] memory (4.2), [51..127] reserved, [127] unused — stay zero.

    RLObservation { vector: vec }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_agent() -> AgentSnapshot {
        AgentSnapshot {
            uid: "A0001-000001".into(),
            pos: [16.0, 10.5],
            heading: 0.0,
            vel: [1.2, 0.0],
            mass_g: 315.0,
            age_years: 2.0,
            energy_kj: 30.0,
            hunger: 0.5,
            fsm_state: avian_core::components::FSMState::Foraging,
            head_offset: [0.0, 0.0],
            alarm_triggered: false,
            sick: false,
            vitality: 0.8,
            memory: vec![],
        }
    }

    #[test]
    fn obs_v1_layout_shape() {
        let obs = state_to_observation(&sample_agent(), &[], &[], &[], 1.0);
        assert_eq!(obs.vector.len(), 128, "obs_v1 must be exactly 128 dims");
        // Normalized pos.
        assert!((obs.vector[0] - (16.0 / 32.0)).abs() < 1e-6);
        assert!((obs.vector[1] - (10.5 / 21.0)).abs() < 1e-6);
        // Heading sin/cos.
        assert!((obs.vector[2] - 1.0).abs() < 1e-6);
        assert!(obs.vector[3].abs() < 1e-6);
        // Velocity magnitude.
        assert!((obs.vector[4] - 1.2).abs() < 1e-6);
        // Energy normalized.
        assert!((obs.vector[5] - (30.0 / 60.0)).abs() < 1e-6);
        // Hunger, age, vitality, light.
        assert!((obs.vector[6] - 0.5).abs() < 1e-6);
        assert!((obs.vector[7] - (2.0 / 15.0)).abs() < 1e-6);
        assert!((obs.vector[8] - 0.8).abs() < 1e-6);
        assert!((obs.vector[9] - 1.0).abs() < 1e-6);
        // Reserved tail must be zero.
        for i in 43..128 {
            assert_eq!(obs.vector[i], 0.0, "reserved slot {i} must be zero");
        }
    }

    #[test]
    fn obs_v1_orders_nearest_grains_and_neighbors() {
        let mut agent = sample_agent();
        agent.pos = [16.0, 10.5];
        // Grains at DISTINCT distances (12→4 m, 21→5 m, 30→~12.6 m) so the
        // nearest-ordering assertion is unambiguous — an exact tie makes the
        // stable `sort_by` order input-dependent.
        let grains = [[21.0, 10.5], [12.0, 10.5], [30.0, 15.0], [50.0, 50.0]];
        let neighbors = [[17.0, 10.5], [30.0, 20.0]];
        let predators = [[10.0, 10.5]];
        let obs = state_to_observation(&agent, &neighbors, &grains, &predators, 1.0);
        // Closest grain (12.0) first: rel pos = [-4, 0].
        assert!((obs.vector[10] + 4.0).abs() < 1e-6, "grain[0].x={}", obs.vector[10]);
        assert!((obs.vector[11]).abs() < 1e-6, "grain[0].y={}", obs.vector[11]);
        // Second closest grain (21.0): rel pos = [5, 0].
        assert!((obs.vector[12] - 5.0).abs() < 1e-6, "grain[1].x={}", obs.vector[12]);
        // Closest neighbor (17.0) first: rel pos = [1, 0], dist 1.
        assert!((obs.vector[16] - 1.0).abs() < 1e-6, "neighbor[0].x={}", obs.vector[16]);
        assert!((obs.vector[18] - 1.0).abs() < 1e-6, "neighbor[0].dist={}", obs.vector[18]);
        // Predator rel pos = [-6, 0], threat = 1/(1+6).
        assert!((obs.vector[37] + 6.0).abs() < 1e-6, "pred.x={}", obs.vector[37]);
        assert!((obs.vector[39] - 1.0 / 7.0).abs() < 1e-6, "threat={}", obs.vector[39]);
    }

    #[test]
    fn reward_components_balance() {
        let dt = 1.0 / 120.0;
        // Well-fed, 4 neighbors, not alarmed, no events: only flocking ticks up.
        let r = RLReward::compute(dt, 40.0, 60.0, 4, false, false, false, false);
        assert!(r.flocking > 0.0);
        assert_eq!(r.grain, 0.0);
        assert_eq!(r.starvation, 0.0);
        assert_eq!(r.captured, 0.0);
        assert_eq!(r.flee_success, 0.0);
        assert!((r.total - 0.1 * dt).abs() < 1e-9, "total={}", r.total);

        // Grain one-shot.
        let r = RLReward::compute(dt, 40.0, 60.0, 0, false, false, true, false);
        assert_eq!(r.grain, 1.0);
        assert_eq!(r.total, 1.0);

        // Starvation pressure (energy < 20% of 60 = 12).
        let r = RLReward::compute(dt, 5.0, 60.0, 0, false, false, false, false);
        assert!((r.starvation + 0.01 * dt).abs() < 1e-12, "starvation={}", r.starvation);

        // Capture dominates.
        let r = RLReward::compute(dt, 5.0, 60.0, 0, true, false, false, true);
        assert_eq!(r.captured, -10.0);
        assert_eq!(r.flee_success, 0.0);

        // Flee success: was alarmed, no longer, not captured.
        let r = RLReward::compute(dt, 40.0, 60.0, 0, false, true, false, false);
        assert_eq!(r.flee_success, 0.5);
        // Not a success if still alarmed.
        let r = RLReward::compute(dt, 40.0, 60.0, 0, true, true, false, false);
        assert_eq!(r.flee_success, 0.0);
    }
}
