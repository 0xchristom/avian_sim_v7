//! 6.2 Metrics dashboard — server-side simulation metrics computed each frame
//! (cheap) and pushed to the viewer every ~100 frames as a `{"type":"metrics"}`
//! WS message. All metrics derive from `SimulationSnapshot`, so they stay in
//! lockstep with what the viewer renders (no second, drifting source of truth).

use avian_core::SimulationSnapshot;
use serde::Serialize;
use std::collections::HashMap;

/// 6.2: aggregate metrics for the dashboard. Every field is `f64` so the JSON
/// payload is uniform and the viewer can format them identically.
#[derive(Clone, Debug, Serialize, Default)]
pub struct Metrics {
    pub frame: u32,
    pub agents: f64,
    pub mean_energy_kj: f64,
    pub mean_hunger: f64,
    pub mean_age_years: f64,
    pub mean_vitality: f64,
    pub flocks: f64,
    pub flocked_agents: f64,
    pub predator_count: f64,
    pub predator_kills: f64,
    pub grains: f64,
    pub spatial_entropy: f64,
    /// Foraging success: grains eaten per m² per simulated second (arena is the
    /// default 32×21 = 672 m²). Rate over the whole run so far.
    pub forage_rate_g_m2_s: f64,
    /// Survival curve: histogram of age-at-death (years), bucket = 1 year.
    pub survival: Vec<(u32, u32)>,
    /// FSM histogram — state name → fraction of agents (sums to 1).
    pub fsm: HashMap<String, f64>,
}

/// 6.2: aggregate a snapshot into dashboard metrics. `predator_kills` is a sim
/// counter (cumulative captures) not present on the snapshot. `world_w`/`world_h`
/// are the arena dimensions (passed in — the snapshot carries no world size).
///
/// Sprint 2 (Audit 5): flock detection is O(N) via a local spatial grid instead
/// of the O(N²) all-pairs scan, and the world dimensions are no longer
/// hard-coded 32×21. Union-find is union-by-size with iterative path halving,
/// so an adversarial insertion order cannot blow the stack (B28).
pub fn compute_metrics(
    snap: &SimulationSnapshot,
    predator_kills: u32,
    grains_consumed: u64,
    death_ages: &[f64],
    world_w: f64,
    world_h: f64,
) -> Metrics {
    let n = snap.agents.len();
    let mut m = Metrics {
        frame: snap.frame,
        agents: n as f64,
        predator_kills: predator_kills as f64,
        ..Metrics::default()
    };

    if n == 0 {
        return m;
    }

    let mut energy_sum = 0.0;
    let mut hunger_sum = 0.0;
    let mut age_sum = 0.0;
    let mut vitality_sum = 0.0;
    // Sprint 2 (Audit 5, B20): aggregate FSM by compact discriminant into a
    // fixed-size array — no per-agent String clone or hash. Converted to the
    // String-keyed histogram only when building the serialized `Metrics.fsm`.
    let mut fsm_counts = [0u32; 9];
    for a in &snap.agents {
        energy_sum += a.energy_kj;
        hunger_sum += a.hunger;
        age_sum += a.age_years;
        vitality_sum += a.vitality;
        fsm_counts[a.fsm_state as u8 as usize] += 1;
    }
    let nf = n as f64;
    m.mean_energy_kj = energy_sum / nf;
    m.mean_hunger = hunger_sum / nf;
    m.mean_age_years = age_sum / nf;
    m.mean_vitality = vitality_sum / nf;
    m.fsm = avian_core::components::FSMState::ALL
        .iter()
        .zip(fsm_counts.iter())
        .filter(|(_, &c)| c > 0)
        .map(|(s, &c)| (s.as_str().to_string(), c as f64 / nf))
        .collect();

    m.predator_count = snap.predators.len() as f64;
    m.grains = snap.grains.len() as f64;

    // 6.2: forage success rate — grains/m²/s. Arena area from the passed-in
    // world dimensions (Sprint 2: no longer hard-coded 32×21).
    let area_m2 = world_w * world_h;
    let elapsed_s = snap.time_us as f64 / 1_000_000.0;
    if elapsed_s > 0.0 {
        m.forage_rate_g_m2_s = grains_consumed as f64 / area_m2 / elapsed_s;
    }

    // 6.2: survival curve — age-at-death histogram in 1-year buckets.
    let mut survival: HashMap<u32, u32> = HashMap::new();
    for &age in death_ages {
        *survival.entry(age.floor() as u32).or_insert(0) += 1;
    }
    let mut buckets: Vec<(u32, u32)> = survival.into_iter().collect();
    buckets.sort_by_key(|&(b, _)| b);
    m.survival = buckets;

    // Flock detection (Sprint 2): union-find over agents within
    // FLOCK_RADIUS_M, counting clusters of >= 4 (the 2.1 acceptance
    // definition). O(N) — each agent only examines cells within one cell of
    // its own instead of all pairs. The DSU is the shared B28 UnionFind
    // (union-by-size + iterative path halving, so no recursion depth limit).
    const FLOCK_RADIUS_M: f64 = 3.0;
    const CELL_M: f64 = FLOCK_RADIUS_M;
    const FLOCK_MIN_SIZE: u32 = 4;
    let mut dsu = crate::union_find::UnionFind::new(n);
    // Bucket agents into cells of CELL_M; cell size ≥ FLOCK_RADIUS_M so two
    // agents within FLOCK_RADIUS_M always share a cell or two 1-cell-apart
    // cells (never skip a needed neighbor).
    let cols = (world_w / CELL_M).ceil().max(1.0) as i64;
    let rows = (world_h / CELL_M).ceil().max(1.0) as i64;
    let mut buckets: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
    for (i, a) in snap.agents.iter().enumerate() {
        let gx = ((a.pos[0] / CELL_M).floor() as i64).clamp(0, cols - 1);
        let gy = ((a.pos[1] / CELL_M).floor() as i64).clamp(0, rows - 1);
        buckets.entry((gx, gy)).or_default().push(i as u32);
    }
    let radius_sq = FLOCK_RADIUS_M * FLOCK_RADIUS_M;
    for (&(gx, gy), members) in buckets.iter() {
        for (idx, &ai) in members.iter().enumerate() {
            let a = &snap.agents[ai as usize];
            for j in members.iter().skip(idx + 1) {
                let b = &snap.agents[*j as usize];
                let dx = a.pos[0] - b.pos[0];
                let dy = a.pos[1] - b.pos[1];
                if dx * dx + dy * dy <= radius_sq {
                    dsu.union(ai, *j);
                }
            }
            // Neighboring cells (Chebyshev distance 1). With cell = radius the
            // two closest cells can still hold in-radius pairs (e.g. both at the
            // shared edge), so all 8 neighbors are checked.
            for dx in -1i64..=1 {
                for dy in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = gx + dx;
                    let ny = gy + dy;
                    if nx < 0 || ny < 0 || nx >= cols || ny >= rows {
                        continue;
                    }
                    if let Some(others) = buckets.get(&(nx, ny)) {
                        for &bj in others {
                            let b = &snap.agents[bj as usize];
                            let ddx = a.pos[0] - b.pos[0];
                            let ddy = a.pos[1] - b.pos[1];
                            if ddx * ddx + ddy * ddy <= radius_sq {
                                dsu.union(ai, bj);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut sizes: HashMap<u32, u32> = HashMap::new();
    for i in 0..n {
        let r = dsu.find(i as u32);
        *sizes.entry(r).or_insert(0) += 1;
    }
    m.flocks = sizes.values().filter(|&&s| s >= FLOCK_MIN_SIZE).count() as f64;
    m.flocked_agents = sizes
        .values()
        .filter(|&&s| s >= FLOCK_MIN_SIZE)
        .sum::<u32>() as f64;

    // Spatial entropy over an 8×8 grid of the world — 1.0 = perfectly uniform,
    // 0.0 = everything in one cell. Uses the passed-in world dimensions.
    let mut cells: Vec<u32> = vec![0; 64];
    for a in &snap.agents {
        let gx = ((a.pos[0].clamp(0.0, world_w) / world_w * 8.0) as usize).min(7);
        let gy = ((a.pos[1].clamp(0.0, world_h) / world_h * 8.0) as usize).min(7);
        cells[gy * 8 + gx] += 1;
    }
    let mut entropy = 0.0;
    for c in cells {
        if c > 0 {
            let p = c as f64 / nf;
            entropy -= p * p.log2();
        }
    }
    // Normalize by the max entropy reachable with `n` agents over 64 cells:
    // log2(min(64, n)) — a handful of agents can never spread over all cells,
    // so dividing by log2(64)≈6 would read as "clumped" even when perfectly
    // uniform. 4 corner agents → 4 occupied cells → entropy log2(4) → 1.0.
    let max_entropy = (n as f64).min(64.0).log2();
    m.spatial_entropy = if max_entropy > 0.0 {
        (entropy / max_entropy).min(1.0)
    } else {
        0.0
    };

    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian_core::components::FSMState;
    use avian_core::AgentSnapshot;

    fn snap_with(agents: Vec<AgentSnapshot>) -> SimulationSnapshot {
        SimulationSnapshot {
            frame: 500,
            time_us: 100_000,
            light_level: 1.0,
            weather: avian_core::components::Weather::Clear,
            weather_intensity: 0.0,
            agents,
            grains: vec![],
            predators: vec![],
            obstacles: vec![],
            agent_count: 0,
            dead_count: 0,
        }
    }

    fn agent(uid: &str, x: f64, y: f64, energy: f64, hunger: f64, fsm: FSMState) -> AgentSnapshot {
        AgentSnapshot {
            uid: uid.into(),
            pos: [x, y],
            heading: 0.0,
            vel: [0.0, 0.0],
            mass_g: 315.0,
            age_years: 2.0,
            energy_kj: energy,
            hunger,
            fsm_state: fsm,
            head_offset: [0.0, 0.0],
            alarm_triggered: false,
            sick: false,
            vitality: 0.8,
            memory: vec![],
        }
    }

    #[test]
    fn aggregates_means_and_fsm_histogram() {
        let m = compute_metrics(
            &snap_with(vec![
                agent("A", 5.0, 5.0, 10.0, 0.2, FSMState::Foraging),
                agent("B", 5.1, 5.0, 30.0, 0.8, FSMState::Spacer),
                agent("C", 5.2, 5.0, 20.0, 0.5, FSMState::Foraging),
            ]),
            3,
            60,
            &[],
            32.0,
            21.0,
        );
        assert_eq!(m.frame, 500);
        assert_eq!(m.agents, 3.0);
        assert!((m.mean_energy_kj - 20.0).abs() < 1e-9);
        assert!((m.mean_hunger - 0.5).abs() < 1e-9);
        assert_eq!(m.predator_kills, 3.0);
        assert!((m.fsm["Foraging"] - 2.0 / 3.0).abs() < 1e-9);
        assert!((m.fsm["Spacer"] - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn detects_a_four_agent_flock() {
        let mut agents = Vec::new();
        for i in 0..4 {
            agents.push(agent(
                &format!("A{i}"),
                10.0 + i as f64 * 0.5,
                10.0,
                10.0,
                0.1,
                FSMState::Foraging,
            ));
        }
        let m = compute_metrics(&snap_with(agents), 0, 0, &[], 32.0, 21.0);
        assert_eq!(m.flocks, 1.0, "a cluster of 4 within 3 m is one flock");
        assert_eq!(m.flocked_agents, 4.0);
    }

    #[test]
    fn no_flock_when_spread_out() {
        let agents = vec![
            agent("A", 2.0, 2.0, 10.0, 0.1, FSMState::Spacer),
            agent("B", 20.0, 2.0, 10.0, 0.1, FSMState::Spacer),
            agent("C", 2.0, 20.0, 10.0, 0.1, FSMState::Spacer),
            agent("D", 20.0, 20.0, 10.0, 0.1, FSMState::Spacer),
        ];
        let m = compute_metrics(&snap_with(agents), 0, 0, &[], 32.0, 21.0);
        assert_eq!(m.flocks, 0.0);
        // Spread corners → near-maximal spatial entropy.
        assert!(m.spatial_entropy > 0.9);
    }

    #[test]
    fn empty_world_is_safe() {
        let m = compute_metrics(&snap_with(vec![]), 0, 0, &[], 32.0, 21.0);
        assert_eq!(m.agents, 0.0);
        assert_eq!(m.mean_energy_kj, 0.0);
        assert_eq!(m.flocks, 0.0);
    }

    #[test]
    fn forage_rate_and_survival_histogram() {
        // 60 grains in 0.1 s over 672 m² → 60 / 672 / 0.1 = 0.892857… /m²/s.
        let m = compute_metrics(
            &snap_with(vec![agent("A", 1.0, 1.0, 10.0, 0.1, FSMState::Foraging)]),
            0,
            60,
            &[2.0, 2.9, 3.1],
            32.0,
            21.0,
        );
        assert!((m.forage_rate_g_m2_s - 60.0 / 672.0 / 0.1).abs() < 1e-9);
        // Buckets: age 2 → 2 entries, age 3 → 1 entry.
        assert_eq!(m.survival, vec![(2, 2), (3, 1)]);
    }

    // Sprint 2 (Audit 5): the O(N) spatial-grid flock detector must agree with
    // the O(N²) all-pairs union-find reference for a random spread including
    // clusters near cell boundaries (the trickiest case for bucketing).
    #[test]
    fn grid_flock_detection_matches_all_pairs() {
        let mut rng = avian_core::rng::SimRng::from_seed(99);
        let mut agents = Vec::new();
        // Dense clusters + scattered points over a 32×21 arena.
        for i in 0..120 {
            let (x, y) = if i % 3 == 0 {
                (
                    10.0 + rng.gen_range(0.0..2.0),
                    10.0 + rng.gen_range(0.0..2.0),
                )
            } else {
                (rng.gen_range(0.5..31.5), rng.gen_range(0.5..20.5))
            };
            agents.push(agent(
                &format!("A{i:03}"),
                x,
                y,
                10.0,
                0.1,
                FSMState::Foraging,
            ));
        }
        let snap = snap_with(agents);
        let n = snap.agents.len();
        let m = compute_metrics(&snap, 0, 0, &[], 32.0, 21.0);

        // O(N²) reference.
        const FLOCK_RADIUS_M: f64 = 3.0;
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut Vec<usize>, x: usize) -> usize {
            if parent[x] != x {
                parent[x] = find(parent, parent[x]);
            }
            parent[x]
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = snap.agents[i].pos[0] - snap.agents[j].pos[0];
                let dy = snap.agents[i].pos[1] - snap.agents[j].pos[1];
                if dx * dx + dy * dy <= FLOCK_RADIUS_M * FLOCK_RADIUS_M {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj {
                        parent[ri] = rj;
                    }
                }
            }
        }
        let mut sizes: HashMap<usize, u32> = HashMap::new();
        for i in 0..n {
            let r = find(&mut parent, i);
            *sizes.entry(r).or_insert(0) += 1;
        }
        let want_flocks = sizes.values().filter(|&&s| s >= 4).count() as f64;
        let want_flocked = sizes.values().filter(|&&s| s >= 4).sum::<u32>() as f64;

        assert_eq!(
            m.flocks, want_flocks,
            "grid flock count differs from all-pairs"
        );
        assert_eq!(m.flocked_agents, want_flocked, "grid flocked count differs");
    }

    // Sprint 2 (Audit 5): a deep chain of unions must not blow the stack with
    // the iterative path-halving DSU (adversarial union order, B28).
    #[test]
    fn union_find_handles_adversarial_chain() {
        let mut agents = Vec::new();
        // 2000 agents in a 1-D line spaced 1.5 m apart → all within 3 m of
        // their neighbors, one long chain. Every pair is a direct union in the
        // grid DSU, so size/rank keeps the tree shallow.
        for i in 0..2000 {
            agents.push(agent(
                &format!("A{i:04}"),
                i as f64 * 1.5,
                10.0,
                10.0,
                0.1,
                FSMState::Foraging,
            ));
        }
        let m = compute_metrics(&snap_with(agents), 0, 0, &[], 32.0, 21.0);
        assert_eq!(m.flocked_agents, 2000.0, "one long chain is one flock");
        assert_eq!(m.flocks, 1.0);
    }
}
