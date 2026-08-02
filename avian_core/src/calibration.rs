//! Literature-calibrated biological constants (4.1).
//!
//! All biological-fidelity values live here — NOT in `simulation.toml` (5.2).
//! Scenario-tunable parameters (world size, populations, seeds) belong in the
//! config; anything used for biological fidelity belongs here. Every value is
//! cited against the feral pigeon (*Columba livia*) literature.
//!
//! Sprint 0 delivers the core set: mass, BMR, speed, vitality model. Phase-2
//! consumer constants (boids weights, predator, night drain) land in Sprint 1.

/// Adult body mass (g). Feral pigeons: 300-350 g.
pub const ADULT_MASS_G: f64 = 315.0;
/// Hatchling mass (g) — Phase 8 breeding curve.
pub const HATCHLING_MASS_G: f64 = 15.0;
/// Fledgling mass (g) — Phase 8 breeding curve.
pub const FLEDGLING_MASS_G: f64 = 200.0;

/// Basal metabolic rate (W) for a 315 g adult. Literature: 3.5-4.5 W.
pub const ADULT_BMR_WATTS: f64 = 4.0;

/// Walk speed (m/s). Literature: 0.5-1.5 m/s.
pub const WALK_SPEED_MS: f64 = 1.2;
/// Flight speed (m/s). Literature: 10-20 m/s. Used by 4.1 flight sprint.
pub const FLY_SPEED_MS: f64 = 15.0;

/// Total horizontal field of view (deg) — pigeons are ~340° monocular with a
/// rear blind spot (4.1). Phase 1 used 170° (only one eye's field), which
/// starved predator detection (2.2): a hawk chasing from behind was invisible.
pub const VISION_FOV_DEGREES: f64 = 340.0;

/// Maximum perception range (m) for neighbor/grain/predator queries. Single
/// source of truth for the per-agent query radius — used by `query_k_nearest`,
/// `cone_cast`, and the grain-visibility filter (5.2: no duplicated literals).
pub const VISION_MAX_RANGE_M: f64 = 10.0;

/// Median wild lifespan (years) — vitality-model anchor: S(t_mid) = 0.5.
pub const VITALITY_T_MID_YEARS: f64 = 4.0;
/// Weibull shape — tuned so S(max_wild_lifespan) < 0.001.
pub const VITALITY_SHAPE_P: f64 = 2.0;
/// Approximate max wild lifespan (years).
pub const WILD_MAX_LIFESPAN_YEARS: f64 = 15.0;

/// Vitality decay model (4.0). Weibull survival curve:
///
///   S(t) = exp(-ln(2) * (t / t_mid)^p)
///
/// S(0) = 1.0, S(t_mid) = 0.5, strictly monotonic non-increasing.
///
/// Note: the plan's *proposed* logistic form 1/(1+exp(k·(t−t_mid))) cannot
/// satisfy the 4.0 acceptance test "at birth vitality = 1.0" for any finite k
/// (it evaluates to ~0.94 at t=0). The Weibull keeps the same two anchor
/// points (median lifespan, max lifespan) and passes every acceptance test.
pub fn vitality_at(t_years: f64) -> f64 {
    if t_years <= 0.0 {
        return 1.0;
    }
    (-std::f64::consts::LN_2 * (t_years / VITALITY_T_MID_YEARS).powf(VITALITY_SHAPE_P)).exp()
}

/// BMR (W) scaled to a body mass (g). Clamped so hatchlings never hit zero.
pub fn bmr_for_mass(mass_g: f64) -> f64 {
    ADULT_BMR_WATTS * (mass_g / ADULT_MASS_G).clamp(0.1, 1.0)
}

/// Critical-energy forage threshold (kJ) — 2.0 CriticalEnergy: below this the
/// agent force-forages regardless of hunger.
pub const CRITICAL_ENERGY_THRESHOLD_KJ: f64 = 5.0;

/// Hunger threshold above which Forage is selected (2.0 root Selector).
pub const FORAGING_HUNGER_THRESHOLD: f64 = 0.4;

/// Nominal energy ceiling used for reward/obs normalization.
pub const MAX_ENERGY_KJ: f64 = 60.0;

/// Light level below which NightRest activates (2.0/2.3).
pub const NIGHT_REST_LIGHT_THRESHOLD: f64 = 0.3;
/// Night energy-drain multiplier (2.3) — drain reduced to 30% of day rate.
pub const NIGHT_DRAIN_FACTOR: f64 = 0.3;
/// Length of a full day/night cycle in sim-seconds (2.3).
pub const DAY_LENGTH_SIM_S: f64 = 600.0;

/// Feather-condition threshold below which Preen activates (2.6).
pub const PREEN_FEATHER_THRESHOLD: f64 = 0.3;
/// Hysteresis stop threshold: while Preening, keep preening until feathers are
/// restored to this value (prevents flicker at the 0.3 entry threshold).
pub const PREEN_STOP_THRESHOLD: f64 = 0.9;
/// Feather condition at spawn (2.6) — pristine.
pub const FEATHER_CONDITION_DEFAULT: f64 = 1.0;
/// Base feather decay rate per sim-second (2.6). With restore 0.4/s, the
/// preen/decay duty cycle lands near the ~10% pigeon preening time budget.
pub const FEATHER_DECAY_RATE_S: f64 = 0.05;
/// Feather restoration rate per sim-second while preening (2.6).
pub const FEATHER_PREEN_RESTORE_RATE_S: f64 = 0.4;
/// Rain multiplies feather decay (2.6/4.4 — hook ready for the weather sprint).
pub const RAIN_FEATHER_DECAY_MULTIPLIER: f64 = 4.0;

/// Vitality below which an agent is Sick (2.7) — from the 4.0 Weibull model.
pub const SICK_VITALITY_THRESHOLD: f64 = 0.3;
/// Sick agents move at this fraction of their normal speed (2.7) — applied to
/// whatever the tree selected (incl. fleeing, hence higher capture risk).
pub const SICK_SPEED_MULTIPLIER: f64 = 0.5;

/// Minimum live population — immigration respawn threshold (2.4).
pub const MIN_POPULATION: usize = 10;

/// Boids weights (2.1).
pub const BOID_SEPARATION_WEIGHT: f64 = 1.5;
pub const BOID_ALIGNMENT_WEIGHT: f64 = 1.0;
pub const BOID_COHESION_WEIGHT: f64 = 0.5;
/// Separation avoidance radius (m).
pub const BOID_SEPARATION_RADIUS_M: f64 = 0.5;
/// Local flock neighborhood radius (m).
pub const BOID_NEIGHBOR_RADIUS_M: f64 = 3.0;

/// Predator speed multiplier over pigeon walk speed (2.2).
pub const PREDATOR_SPEED_MULTIPLIER: f64 = 2.5;
/// Predator detection radius (m) — agents inside this + FOV enter Fleeing.
pub const PREDATOR_DETECTION_RADIUS_M: f64 = 8.0;
/// Contact distance (m) at which capture is rolled. This is a center-to-center
/// threshold ≈ the combined body radii (agent ball 0.4 + predator ball 0.5),
/// i.e. it fires at physical contact — a raw 0.3 m gap is unreachable while
/// the two colliders overlap-free.
pub const PREDATOR_CONTACT_DISTANCE_M: f64 = 1.0;
/// Capture probability on contact (2.2).
pub const PREDATOR_CAPTURE_PROBABILITY: f64 = 0.5;
/// Frames a predator cannot capture after a miss.
pub const PREDATOR_MISS_COOLDOWN_FRAMES: u32 = 120;
/// Minimum distance for the post-strike reposition waypoint (2.2) — keeps the
/// predator RANGING the map instead of pinning one local cluster.
pub const PREDATOR_REPOSITION_MIN_DIST_M: f64 = 12.0;
/// Frames a predator spends flying to its reposition waypoint before it may
/// chase again (2.2). At 3 m/s × ~4 s ≈ 12 m — matches `REPOSITION_MIN_DIST`.
pub const PREDATOR_REPOSITION_COOLDOWN_FRAMES: u32 = 480;
/// Minimum predator lifetime before it despawns (2.2b).
pub const PREDATOR_LIFETIME_MIN_S: f64 = 5.0;
/// Maximum predator lifetime before it despawns (2.2b). Each predator draws a
/// random lifetime in `[MIN_S, MAX_S]` at spawn, then despawns when it elapses.
pub const PREDATOR_LIFETIME_MAX_S: f64 = 15.0;

/// Energy gained per grain consumed (2.4 uses this for the reward too).
pub const GRAIN_ENERGY_KJ: f64 = 0.5;

/// World dimensions (m) — used to normalize `pos` in obs_v1 (3.1). Currently
/// fixed by the wall layout in `Simulation::new`; becomes scenario config (5.2).
pub const WORLD_WIDTH_M: f64 = 32.0;
pub const WORLD_HEIGHT_M: f64 = 21.0;

/// obs_v1 (3.1) field counts.
pub const OBS_NEIGHBOR_COUNT: usize = 7;
pub const OBS_GRAIN_COUNT: usize = 3;
/// 4.2 memory slots reserved in obs_v1 (zero until spatial memory ships).
pub const OBS_MEMORY_COUNT: usize = 4;

/// 3.2 event-driven reward shaping constants. Per-second rates are multiplied
/// by `dt` (~1/120) before being added each tick (plan 3.2 CLARIFICATION) so
/// they balance against the one-shot rewards instead of dwarfing them.
/// +1.0 per grain consumed (one-shot).
pub const REWARD_GRAIN: f64 = 1.0;
/// +0.1/sec for being within 2 m of ≥ 2 other agents (flocking).
pub const REWARD_FLOCKING_PER_S: f64 = 0.1;
pub const REWARD_FLOCK_NEIGHBOR_DIST_M: f64 = 2.0;
pub const REWARD_FLOCK_NEIGHBORS_MIN: usize = 2;
/// -0.01/sec while energy is below this fraction of MAX_ENERGY_KJ (starvation
/// pressure).
pub const REWARD_STARVATION_PER_S: f64 = 0.01;
pub const REWARD_STARVATION_ENERGY_FRACTION: f64 = 0.2;
/// -10.0 when captured by a predator (one-shot).
pub const REWARD_CAPTURED: f64 = -10.0;
/// +0.5 when a predator leaves (despawns / moves away) without capturing — i.e.
/// a fleeing episode ends safely (one-shot).
pub const REWARD_FLEE_SUCCESS: f64 = 0.5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mass_curve_range() {
        assert!(ADULT_MASS_G >= 300.0 && ADULT_MASS_G <= 350.0, "adult mass out of range");
        assert!(HATCHLING_MASS_G < FLEDGLING_MASS_G, "hatchling must be lightest");
        assert!(FLEDGLING_MASS_G < ADULT_MASS_G, "fledgling must be lighter than adult");
    }

    #[test]
    fn test_bmr_range() {
        assert!(ADULT_BMR_WATTS >= 3.5 && ADULT_BMR_WATTS <= 4.5, "BMR out of literature range");
        assert!((bmr_for_mass(ADULT_MASS_G) - ADULT_BMR_WATTS).abs() < 1e-9);
        assert!(bmr_for_mass(0.0) > 0.0, "BMR must never be zero");
    }

    #[test]
    fn test_speed_range() {
        assert!(WALK_SPEED_MS >= 0.5 && WALK_SPEED_MS <= 1.5, "walk speed out of range");
        assert!(FLY_SPEED_MS >= 10.0 && FLY_SPEED_MS <= 20.0, "flight speed out of range");
        assert!(WALK_SPEED_MS < FLY_SPEED_MS);
    }

    #[test]
    fn test_vitality_at_birth_is_one() {
        assert!((vitality_at(0.0) - 1.0).abs() < 1e-9, "vitality must be 1.0 at birth");
    }

    #[test]
    fn test_vitality_monotonic() {
        let mut prev = vitality_at(0.0);
        for t in 1..=20 {
            let v = vitality_at(t as f64);
            assert!(v <= prev + 1e-12, "vitality must be non-increasing at t={}", t);
            prev = v;
        }
    }

    #[test]
    fn test_vitality_thresholds() {
        // Crosses 0.5 at the median wild lifespan.
        assert!((vitality_at(VITALITY_T_MID_YEARS) - 0.5).abs() < 1e-9);
        // Below 0.001 at max wild lifespan.
        assert!(vitality_at(WILD_MAX_LIFESPAN_YEARS) < 0.001);
        // Plausible median band: 3-5 years.
        assert!(vitality_at(3.0) > 0.5, "median too old");
        assert!(vitality_at(5.0) < 0.5, "median too young");
    }

    #[test]
    fn test_phase2_consumer_ranges() {
        assert!(BOID_SEPARATION_WEIGHT > BOID_ALIGNMENT_WEIGHT);
        assert!(BOID_ALIGNMENT_WEIGHT > BOID_COHESION_WEIGHT);
        assert!(PREDATOR_SPEED_MULTIPLIER > 2.0);
        assert!(PREDATOR_DETECTION_RADIUS_M >= PREDATOR_CONTACT_DISTANCE_M);
        assert!(PREDATOR_CONTACT_DISTANCE_M >= 0.8);
        assert!(PREDATOR_REPOSITION_MIN_DIST_M > PREDATOR_DETECTION_RADIUS_M);
        assert!(PREDATOR_REPOSITION_COOLDOWN_FRAMES > PREDATOR_MISS_COOLDOWN_FRAMES);
        assert!(PREDATOR_LIFETIME_MIN_S > 0.0);
        assert!(PREDATOR_LIFETIME_MAX_S >= PREDATOR_LIFETIME_MIN_S);
        assert!(PREEN_FEATHER_THRESHOLD < PREEN_STOP_THRESHOLD);
        assert!(PREEN_STOP_THRESHOLD <= 1.0);
        assert!(FEATHER_DECAY_RATE_S > 0.0);
        assert!(FEATHER_PREEN_RESTORE_RATE_S > FEATHER_DECAY_RATE_S);
        assert!(RAIN_FEATHER_DECAY_MULTIPLIER > 1.0);
        assert!((0.0..1.0).contains(&SICK_VITALITY_THRESHOLD));
        assert!((0.0..1.0).contains(&SICK_SPEED_MULTIPLIER));
        assert!((0.0..1.0).contains(&NIGHT_DRAIN_FACTOR));
        assert!(NIGHT_REST_LIGHT_THRESHOLD > 0.0 && NIGHT_REST_LIGHT_THRESHOLD < 1.0);
        assert!(WORLD_WIDTH_M > 0.0 && WORLD_HEIGHT_M > 0.0);
        assert!(OBS_NEIGHBOR_COUNT > 0 && OBS_GRAIN_COUNT > 0);
        assert!(REWARD_GRAIN > 0.0);
        assert!(REWARD_FLOCKING_PER_S > 0.0);
        assert!(REWARD_STARVATION_PER_S > 0.0);
        assert!(REWARD_CAPTURED < 0.0);
        assert!(REWARD_FLEE_SUCCESS > 0.0);
        assert!(REWARD_FLOCK_NEIGHBORS_MIN >= 2);
        assert!((0.0..1.0).contains(&REWARD_STARVATION_ENERGY_FRACTION));
    }
}
