//! Literature-calibrated biological constants (4.1).
//!
//! All biological-fidelity values live here — NOT in `simulation.toml` (5.2).
//! Scenario-tunable parameters (world size, populations, seeds) belong in the
//! config; anything used for biological fidelity belongs here. Every value is
//! cited against the feral pigeon (*Columba livia*) literature.
//!
//! Sprint 0 delivers the core set: mass, BMR, speed, vitality model. Phase-2
//! consumer constants (boids weights, predator, night drain) land in Sprint 1.

use crate::components::Weather;

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

/// Flight metabolic-rate multiplier over BMR (4.1). Real pigeon flight costs
/// ≈ 6-8× resting metabolism (a 315 g pigeon at ~20-25 W vs ~4 W BMR). Applied
/// while the agent is airborne (speed above `FLIGHT_SPEED_THRESHOLD_MS`).
pub const FLIGHT_MR_MULTIPLIER: f64 = 7.0;
/// Speed (m/s) above which the agent is treated as flying — gates the flight
/// metabolic cost. Midway between walk (1.2) and fly (15.0): a sick pigeon
/// fleeing at half flight speed (7.5) still counts as flying.
pub const FLIGHT_SPEED_THRESHOLD_MS: f64 = 5.0;

/// Phase 9 (Audit 3): depth (m) of the invisible thermal strip that forms on
/// the sun-facing side of a Building. ~2.5 m keeps the updraft zone just off
/// the wall so birds ride it without colliding with the building collider.
pub const THERMAL_DEPTH_M: f64 = 2.5;

/// Phase 9 (Audit 3): metabolic-rate multiplier while `FSMState::Gliding`.
/// Near-zero — a gliding pigeon exploits the updraft instead of flapping, so
/// its flight cost collapses from `FLIGHT_MR_MULTIPLIER` (7×) to ~0.15× BMR
/// (still breathing, still thermoregulating, but no flapping power).
pub const GLIDE_MR_MULTIPLIER: f64 = 0.15;

/// Phase 9 (Audit 3): cruising speed (m/s) while gliding a thermal. Soaring
/// is slower than active flapping (15) but well above the walking speed and
/// above `FLIGHT_SPEED_THRESHOLD_MS`, so the bird is genuinely airborne.
pub const GLIDE_SPEED_MS: f64 = 8.0;

/// Phase 9 (Audit 3): the bird's heading must be within this angle (degrees) of
/// the thermal's updraft `flow` vector to enter Gliding.
pub const GLIDE_HEADING_ALIGN_DEG: f64 = 30.0;

/// Phase 9 (Audit 3): while gliding, boids steering (the "maneuvering" force)
/// is multiplied by this factor — the bird rides the updraft in a straight-ish
/// line and cannot bank hard into the flock.
pub const GLIDE_STEERING_MULTIPLIER: f64 = 0.2;

/// Daily energy requirement (kJ) for a 315 g adult (4.1). Literature: 100-200 kJ.
pub const DAILY_ENERGY_REQUIREMENT_KJ: f64 = 150.0;

/// Binocular overlap (deg) in front of the head (4.1). Pigeons have ~25° of
/// forward binocular overlap for depth perception; the monocular fields cover
/// ~340° total (VISION_FOV_DEGREES). Refines the `Vision` blind-spot layout.
pub const BINOCULAR_OVERLAP_DEGREES: f64 = 25.0;

/// Total horizontal field of view (deg) — pigeons are ~340° monocular with a
/// rear blind spot (4.1). Phase 1 used 170° (only one eye's field), which
/// starved predator detection (2.2): a hawk chasing from behind was invisible.
pub const VISION_FOV_DEGREES: f64 = 340.0;

/// Maximum perception range (m) for neighbor/grain/predator queries. Single
/// source of truth for the per-agent query radius — used by `query_k_nearest`,
/// `cone_cast`, and the grain-visibility filter (5.2: no duplicated literals).
pub const VISION_MAX_RANGE_M: f64 = 10.0;

/// 4.3: max rejection samples when drawing a spawn/patrol point that must not
/// land inside an obstacle. With the default urban map (5 boxes over a
/// 28×17 free area) this succeeds on the first or second try almost always.
pub const MAX_FREE_POINT_TRIES: u32 = 32;

/// Sprint 2 (Audit 5, B10): a velocity change below this magnitude (m/s) is
/// "not material" — the physics hot loop skips `set_linvel` (and the wake-up
/// it implies) so sleeping/idle bodies are not force-woken every tick. 1 cm/s
/// is far below the slowest agent speed (~2 m/s walking) while still small
/// enough that a genuinely moving body always crosses it.
pub const BODY_VELOCITY_WAKE_EPS: f32 = 0.01;

/// 4.3: line-of-sight tolerance (fraction of the cast). A static hit at
/// toi >= 1 - EPS means the target itself is touching the obstacle boundary,
/// not that an obstacle blocks the sight line.
pub const LOS_BLOCK_EPS: f64 = 1e-4;

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

/// Flight metabolic multiplier for a given speed (4.1): `FLIGHT_MR_MULTIPLIER`
/// while airborne (`v_mag >= FLIGHT_SPEED_THRESHOLD_MS`), else 1.0 (ground).
/// Applied to BMR in both the energy-balance drain sites (`metabolism_system`
/// and its inline mirror in `run_systems`) so conservation stays exact.
pub fn flight_mr_multiplier(v_mag: f64) -> f64 {
    if v_mag >= FLIGHT_SPEED_THRESHOLD_MS {
        FLIGHT_MR_MULTIPLIER
    } else {
        1.0
    }
}

/// Phase 9 (Audit 3): same as `flight_mr_multiplier` but a Gliding bird pays
/// `GLIDE_MR_MULTIPLIER` instead of the full flapping cost. Used BOTH by
/// `metabolism_system` and the inline mirror in `run_systems` so the 7.2
/// energy-balance accounting stays exact across the two drains.
pub fn flight_mr_multiplier_state(v_mag: f64, gliding: bool) -> f64 {
    if gliding {
        GLIDE_MR_MULTIPLIER
    } else {
        flight_mr_multiplier(v_mag)
    }
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
/// Audit 4 §9.5: fraction of night-eligible birds that stay in `Scanning` as
/// flock sentinels instead of `Roosting`. Drawn per-tick from `sim.rng` so the
/// same birds are never sentinels every night.
pub const SENTINEL_FRACTION: f64 = 0.12;
/// Audit 4 §9.5: sentinel patrol speed as a fraction of `max_speed_ms` (slow,
/// alert shuffle while the rest of the flock sleeps).
pub const SENTINEL_PATROL_SPEED_FRACTION: f64 = 0.3;
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

// ---------------------------------------------------------------------------
// 4.4 Weather.
// ---------------------------------------------------------------------------
/// Weather re-roll cadence (frames) — the stochastic scheduler re-draws the
/// global weather state every this many frames.
pub const WEATHER_UPDATE_INTERVAL_FRAMES: u32 = 600;
/// Ramp rate (per sim-second) of `weather_intensity` — a full 0↔1 transition
/// takes 1.0 s, so weather effects fade in/out instead of snapping.
pub const WEATHER_RAMP_RATE_PER_S: f64 = 1.0;
/// Rain cuts vision range to this fraction of `VISION_MAX_RANGE_M` (wet
/// feathers, heavy overcast).
pub const RAIN_VISIBILITY_FACTOR: f64 = 0.6;
/// Wind drift speed (m/s) added to every body's velocity while Wind is active.
pub const WIND_SPEED_MS: f64 = 4.0;
/// Wind multiplies the flight metabolic rate — flying in wind costs more.
pub const WIND_FLIGHT_MR_MULTIPLIER: f64 = 1.5;
/// Heat multiplies basal metabolism (water-need proxy: no dedicated thirst
/// channel in v7, so faster dehydration shows up as faster energy burn).
pub const HEAT_BMR_MULTIPLIER: f64 = 1.25;

/// 4.4: vision-range multiplier for the current weather (1.0 except rain).
pub fn weather_vision_scale(weather: Weather, intensity: f64) -> f64 {
    match weather {
        Weather::Rain => 1.0 - (1.0 - RAIN_VISIBILITY_FACTOR) * intensity.clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// 4.4: basal-metabolism multiplier for the current weather (heat only).
pub fn weather_metabolic_multiplier(weather: Weather, intensity: f64) -> f64 {
    match weather {
        Weather::Heat => 1.0 + (HEAT_BMR_MULTIPLIER - 1.0) * intensity.clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// 4.4: flight-metabolism multiplier for the current weather (wind only).
pub fn weather_wind_flight_multiplier(weather: Weather, intensity: f64) -> f64 {
    match weather {
        Weather::Wind => 1.0 + (WIND_FLIGHT_MR_MULTIPLIER - 1.0) * intensity.clamp(0.0, 1.0),
        _ => 1.0,
    }
}

/// Vitality below which an agent is Sick (2.7) — from the 4.0 Weibull model.
pub const SICK_VITALITY_THRESHOLD: f64 = 0.3;
/// Sick agents move at this fraction of their normal speed (2.7) — applied to
/// whatever the tree selected (incl. fleeing, hence higher capture risk).
pub const SICK_SPEED_MULTIPLIER: f64 = 0.5;

/// Minimum live population — immigration respawn threshold (2.4).
pub const MIN_POPULATION: usize = 10;

/// 4.2 spatial memory.
/// Maximum remembered food locations per agent (LRU-evicted at this cap).
pub const MEMORY_SLOTS_MAX: usize = 8;
/// Frames a remembered food location lasts before decaying to zero strength.
/// At 60 fps ≈ 10 s — enough to revisit a depleted patch that re-seeds.
pub const MEMORY_DECAY_FRAMES: u32 = 600;
/// Distance (m) at which food is committed to memory ("food found within
/// 0.5 m", matching the consumption radius).
pub const MEMORY_FOUND_DIST_M: f64 = 0.5;
/// Minimum memory strength for a slot to be a viable forage target; slots
/// below this are forgotten.
pub const MEMORY_MIN_STRENGTH: f64 = 0.05;

/// Boids weights (2.1).
pub const BOID_SEPARATION_WEIGHT: f64 = 1.5;
pub const BOID_ALIGNMENT_WEIGHT: f64 = 1.0;
pub const BOID_COHESION_WEIGHT: f64 = 0.5;
/// Separation avoidance radius (m).
pub const BOID_SEPARATION_RADIUS_M: f64 = 0.5;
/// Local flock neighborhood radius (m).
pub const BOID_NEIGHBOR_RADIUS_M: f64 = 3.0;

/// Audit 4 §9.8: maximum Lévy "relocate" step (m) for the Wander spacer. The
/// raw `levy_step` tail is heavy (P(step > s) ≈ s^-2), but the call sites cap
/// it — the old hardcoded 5 m cap meant a wanderer could never escape a
/// cluster's gravity well (neighbor radius 3 m + cohesion pull). 15 m ≈ half
/// the arena width: rare, genuinely arena-crossing excursions that let an
/// individual search away from the group. The flightless step is still walked
/// at Wander speed (~0.8×), so a 15 m step is a ~15 s directional excursion,
/// never a teleport.
pub const WANDER_LEVY_MAX_STEP_M: f64 = 15.0;

/// Audit 3 (Phase 2) — targeted caching thresholds. All frame-based, so
/// determinism (7.1) is preserved: caches are keyed by hecs entity (generation
/// aware) and invalidated purely by simulation state deltas, never wall-clock.
/// Position drift (m) before an agent's cached visible-grain list is stale.
pub const GRAIN_VIS_CACHE_POS_EPS: f64 = 0.25;
/// Heading drift (rad, ≈5.7°) before the FOV-cone result is stale.
pub const GRAIN_VIS_CACHE_ANGLE_EPS: f64 = 0.1;
/// Vision-range drift (m) before the cache is stale (weather transitions).
pub const GRAIN_VIS_CACHE_RANGE_EPS: f64 = 0.05;
/// Neighbor-set refresh period for a stable flock (frames).
pub const NEIGHBOR_REFRESH_FRAMES: u32 = 4;
/// A flock counts as "dense" once it has at least this many neighbors
/// (of the k=7 neighborhood query). High bar: only genuinely dense, coherent
/// flocks get throttled; sparse foragers refresh every frame.
pub const NEIGHBOR_STABLE_MIN_COUNT: usize = 6;
/// Per-frame velocity delta (m/s) below which the flock is "stable".
pub const NEIGHBOR_STABLE_VEL_EPS: f64 = 0.2;
/// Every N frames, prune stale phase-2 cache entries for despawned entities.
pub const CACHE_PRUNE_FRAMES: u32 = 600;

/// Predator speed multiplier over pigeon walk speed (2.2). Kept for the
/// v1 ground-sprint era; 4.1 recalibrates the predator's absolute speed via
/// `PREDATOR_SPEED_MS` so it can catch a fleeing (now flying) pigeon.
pub const PREDATOR_SPEED_MULTIPLIER: f64 = 2.5;
/// Predator absolute speed (m/s) — 4.1 v2 recalibration. Pigeons now flee by
/// flying at `FLY_SPEED_MS` (15); a sick pigeon flees at half that (7.5).
/// `PREDATOR_SPEED_MS = 10` sits between the two: healthy pigeons that detect
/// the hawk early escape (flee_success is real), while sick pigeons — and any
/// pigeon caught in the rear blind spot — are run down, matching the 2.7
/// "sick captured first" acceptance.
pub const PREDATOR_SPEED_MS: f64 = 10.0;
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

/// 6.2 hunt-state machine: how long a predator stays BUSY (halted) after a
/// strike or a miss before it re-engages/patrols. The "catches it — busy 1 s"
/// beat from the request.
pub const PREDATOR_CATCH_BUSY_S: f64 = 1.0;
/// Dynamic speed scale bounds (1 = slow, 5 = very fast). The predator's
/// `speed_level` lives in `[MIN, MAX]`; chase ramps it UP, await decays it
/// DOWN. Level N maps to `PREDATOR_SPEED_MS * N / MAX`.
pub const PREDATOR_SPEED_LEVEL_MIN: u8 = 1;
pub const PREDATOR_SPEED_LEVEL_MAX: u8 = 5;
/// Chase acceleration (speed levels per sim-second) — a hawk ramps to full
/// speed (level 5) in `(MAX-MIN)/RAMP ≈ 0.5 s` of pursuit.
pub const PREDATOR_SPEED_RAMP_LEVELS_PER_S: f64 = 8.0;
/// Await deceleration (speed levels per sim-second) — returning to a slow
/// patrol (level 1) when no prey is in range.
pub const PREDATOR_SPEED_DECAY_LEVELS_PER_S: f64 = 4.0;

/// 6.2: meals (captures) a predator needs before it despawns automatically,
/// per the "predator disappears after eating 3 pigeons" request.
pub const PREDATOR_FILL_MEALS_TARGET: u32 = 3;

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

// ---------------------------------------------------------------------------
// 6.3 calibration export — single source of truth for the Python analysis.
// ---------------------------------------------------------------------------
/// 6.3: export every calibration constant + sampled helper outputs to JSON so
/// `analysis/check_biology.py` reads the SAME values as the compiled sim — no
/// second hardcoded copy that can drift. The `calibration_export_matches` test
/// below regenerates `analysis/calibration_export.json` and fails the release
/// gate if it differs from the committed file.
pub fn calibration_export_json() -> serde_json::Value {
    serde_json::json!({
        "adult_mass_g": ADULT_MASS_G,
        "hatchling_mass_g": HATCHLING_MASS_G,
        "fledgling_mass_g": FLEDGLING_MASS_G,
        "adult_bmr_watts": ADULT_BMR_WATTS,
        "walk_speed_ms": WALK_SPEED_MS,
        "fly_speed_ms": FLY_SPEED_MS,
        "flight_mr_multiplier": FLIGHT_MR_MULTIPLIER,
        "flight_speed_threshold_ms": FLIGHT_SPEED_THRESHOLD_MS,
        "thermal_depth_m": THERMAL_DEPTH_M,
        "glide_mr_multiplier": GLIDE_MR_MULTIPLIER,
        "glide_speed_ms": GLIDE_SPEED_MS,
        "glide_heading_align_deg": GLIDE_HEADING_ALIGN_DEG,
        "glide_steering_multiplier": GLIDE_STEERING_MULTIPLIER,
        "daily_energy_requirement_kj": DAILY_ENERGY_REQUIREMENT_KJ,
        "binocular_overlap_degrees": BINOCULAR_OVERLAP_DEGREES,
        "vision_fov_degrees": VISION_FOV_DEGREES,
        "vision_max_range_m": VISION_MAX_RANGE_M,
        "vitality_t_mid_years": VITALITY_T_MID_YEARS,
        "vitality_shape_p": VITALITY_SHAPE_P,
        "wild_max_lifespan_years": WILD_MAX_LIFESPAN_YEARS,
        "sick_vitality_threshold": SICK_VITALITY_THRESHOLD,
        "sick_speed_multiplier": SICK_SPEED_MULTIPLIER,
        "critical_energy_threshold_kj": CRITICAL_ENERGY_THRESHOLD_KJ,
        "foraging_hunger_threshold": FORAGING_HUNGER_THRESHOLD,
        "max_energy_kj": MAX_ENERGY_KJ,
        "night_rest_light_threshold": NIGHT_REST_LIGHT_THRESHOLD,
        "night_drain_factor": NIGHT_DRAIN_FACTOR,
        "sentinel_fraction": SENTINEL_FRACTION,
        "sentinel_patrol_speed_fraction": SENTINEL_PATROL_SPEED_FRACTION,
        "wander_levy_max_step_m": WANDER_LEVY_MAX_STEP_M,
        "day_length_sim_s": DAY_LENGTH_SIM_S,
        "preen_feather_threshold": PREEN_FEATHER_THRESHOLD,
        "preen_stop_threshold": PREEN_STOP_THRESHOLD,
        "feather_decay_rate_s": FEATHER_DECAY_RATE_S,
        "feather_preen_restore_rate_s": FEATHER_PREEN_RESTORE_RATE_S,
        "rain_feather_decay_multiplier": RAIN_FEATHER_DECAY_MULTIPLIER,
        "rain_visibility_factor": RAIN_VISIBILITY_FACTOR,
        "wind_speed_ms": WIND_SPEED_MS,
        "wind_flight_mr_multiplier": WIND_FLIGHT_MR_MULTIPLIER,
        "heat_bmr_multiplier": HEAT_BMR_MULTIPLIER,
        "memory_slots_max": MEMORY_SLOTS_MAX,
        "memory_decay_frames": MEMORY_DECAY_FRAMES,
        "memory_found_dist_m": MEMORY_FOUND_DIST_M,
        "boid_separation_weight": BOID_SEPARATION_WEIGHT,
        "boid_alignment_weight": BOID_ALIGNMENT_WEIGHT,
        "boid_cohesion_weight": BOID_COHESION_WEIGHT,
        "boid_separation_radius_m": BOID_SEPARATION_RADIUS_M,
        "boid_neighbor_radius_m": BOID_NEIGHBOR_RADIUS_M,
        "predator_speed_ms": PREDATOR_SPEED_MS,
        "predator_detection_radius_m": PREDATOR_DETECTION_RADIUS_M,
        "predator_contact_distance_m": PREDATOR_CONTACT_DISTANCE_M,
        "predator_capture_probability": PREDATOR_CAPTURE_PROBABILITY,
        "predator_lifetime_min_s": PREDATOR_LIFETIME_MIN_S,
        "predator_lifetime_max_s": PREDATOR_LIFETIME_MAX_S,
        "predator_catch_busy_s": PREDATOR_CATCH_BUSY_S,
        "predator_speed_level_min": PREDATOR_SPEED_LEVEL_MIN,
        "predator_speed_level_max": PREDATOR_SPEED_LEVEL_MAX,
        "predator_fill_meals_target": PREDATOR_FILL_MEALS_TARGET,
        "grain_energy_kj": GRAIN_ENERGY_KJ,
        "world_width_m": WORLD_WIDTH_M,
        "world_height_m": WORLD_HEIGHT_M,
        "obs_neighbor_count": OBS_NEIGHBOR_COUNT,
        "obs_grain_count": OBS_GRAIN_COUNT,
        "obs_memory_count": OBS_MEMORY_COUNT,
        "reward_grain": REWARD_GRAIN,
        "reward_flocking_per_s": REWARD_FLOCKING_PER_S,
        "reward_starvation_per_s": REWARD_STARVATION_PER_S,
        "reward_captured": REWARD_CAPTURED,
        "reward_flee_success": REWARD_FLEE_SUCCESS,
        "min_population": MIN_POPULATION,
        // Sampled helper outputs so check_biology.py can verify them without
        // re-implementing the curves.
        "sampled": {
            "vitality_at_0": vitality_at(0.0),
            "vitality_at_t_mid": vitality_at(VITALITY_T_MID_YEARS),
            "vitality_at_max": vitality_at(WILD_MAX_LIFESPAN_YEARS),
            "bmr_at_adult": bmr_for_mass(ADULT_MASS_G),
            "flight_mr_walking": flight_mr_multiplier(WALK_SPEED_MS),
            "flight_mr_flying": flight_mr_multiplier(FLY_SPEED_MS),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mass_curve_range() {
        assert!(
            ADULT_MASS_G >= 300.0 && ADULT_MASS_G <= 350.0,
            "adult mass out of range"
        );
        assert!(
            HATCHLING_MASS_G < FLEDGLING_MASS_G,
            "hatchling must be lightest"
        );
        assert!(
            FLEDGLING_MASS_G < ADULT_MASS_G,
            "fledgling must be lighter than adult"
        );
    }

    #[test]
    fn test_bmr_range() {
        assert!(
            ADULT_BMR_WATTS >= 3.5 && ADULT_BMR_WATTS <= 4.5,
            "BMR out of literature range"
        );
        assert!((bmr_for_mass(ADULT_MASS_G) - ADULT_BMR_WATTS).abs() < 1e-9);
        assert!(bmr_for_mass(0.0) > 0.0, "BMR must never be zero");
    }

    #[test]
    fn test_speed_range() {
        assert!(
            WALK_SPEED_MS >= 0.5 && WALK_SPEED_MS <= 1.5,
            "walk speed out of range"
        );
        assert!(
            FLY_SPEED_MS >= 10.0 && FLY_SPEED_MS <= 20.0,
            "flight speed out of range"
        );
        assert!(WALK_SPEED_MS < FLY_SPEED_MS);
        assert!(
            FLIGHT_MR_MULTIPLIER > 1.0,
            "flight must cost more than rest"
        );
        assert!(
            FLIGHT_MR_MULTIPLIER >= 5.0 && FLIGHT_MR_MULTIPLIER <= 9.0,
            "flight MR 6-8x literature"
        );
        // Flight threshold between walk and half-sick-flee (7.5) so a fleeing
        // pigeon — healthy (15) or sick (7.5) — always pays the flight cost.
        assert!(FLIGHT_SPEED_THRESHOLD_MS < FLY_SPEED_MS * 0.5);
        assert!(FLIGHT_SPEED_THRESHOLD_MS > WALK_SPEED_MS);
        assert_eq!(
            flight_mr_multiplier(WALK_SPEED_MS),
            1.0,
            "walking must not pay flight cost"
        );
        assert_eq!(flight_mr_multiplier(FLY_SPEED_MS), FLIGHT_MR_MULTIPLIER);
        assert_eq!(
            flight_mr_multiplier(FLY_SPEED_MS * 0.5),
            FLIGHT_MR_MULTIPLIER,
            "sick half-speed flee still counts as flying"
        );
    }

    #[test]
    fn test_energy_requirements() {
        assert!(
            (100.0..=200.0).contains(&DAILY_ENERGY_REQUIREMENT_KJ),
            "daily requirement outside literature 100-200 kJ"
        );
    }

    #[test]
    fn test_vision_refinements() {
        // Binocular overlap ~25 deg in front; FOV must swallow it comfortably.
        assert!((15.0..=35.0).contains(&BINOCULAR_OVERLAP_DEGREES));
        assert!(BINOCULAR_OVERLAP_DEGREES < VISION_FOV_DEGREES);
    }

    #[test]
    fn test_vitality_at_birth_is_one() {
        assert!(
            (vitality_at(0.0) - 1.0).abs() < 1e-9,
            "vitality must be 1.0 at birth"
        );
    }

    #[test]
    fn test_vitality_monotonic() {
        let mut prev = vitality_at(0.0);
        for t in 1..=20 {
            let v = vitality_at(t as f64);
            assert!(
                v <= prev + 1e-12,
                "vitality must be non-increasing at t={}",
                t
            );
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
        // 4.1 v2: predator must be able to run down a sick pigeon (half flight
        // speed = 7.5) but NOT outrun a healthy pigeon that detects it early
        // (full flight = 15) — otherwise fleeing is pointless.
        assert!(PREDATOR_SPEED_MS > FLY_SPEED_MS * 0.5);
        assert!(PREDATOR_SPEED_MS < FLY_SPEED_MS);
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
        // 4.2 spatial memory: sane bounds.
        assert!(MEMORY_SLOTS_MAX >= 1);
        assert!(MEMORY_DECAY_FRAMES > 0);
        assert!(MEMORY_FOUND_DIST_M > 0.0 && MEMORY_FOUND_DIST_M <= 0.5);
        assert!((0.0..1.0).contains(&MEMORY_MIN_STRENGTH));
        // 4.4 weather: sane bounds + smooth helpers.
        assert!((0.0..1.0).contains(&RAIN_VISIBILITY_FACTOR));
        assert!(WIND_SPEED_MS > 0.0);
        assert!(WIND_FLIGHT_MR_MULTIPLIER > 1.0);
        assert!(HEAT_BMR_MULTIPLIER > 1.0);
        assert!(WEATHER_RAMP_RATE_PER_S > 0.0);
        assert!(WEATHER_UPDATE_INTERVAL_FRAMES > 0);
        assert!((weather_vision_scale(Weather::Clear, 1.0) - 1.0).abs() < 1e-12);
        assert!((weather_vision_scale(Weather::Rain, 0.0) - 1.0).abs() < 1e-12);
        assert!((weather_vision_scale(Weather::Rain, 1.0) - RAIN_VISIBILITY_FACTOR).abs() < 1e-12);
        assert!(
            (weather_metabolic_multiplier(Weather::Heat, 1.0) - HEAT_BMR_MULTIPLIER).abs() < 1e-12
        );
        assert!((weather_metabolic_multiplier(Weather::Clear, 1.0) - 1.0).abs() < 1e-12);
        assert!(
            (weather_wind_flight_multiplier(Weather::Wind, 1.0) - WIND_FLIGHT_MR_MULTIPLIER).abs()
                < 1e-12
        );
        assert!((weather_wind_flight_multiplier(Weather::Wind, 0.0) - 1.0).abs() < 1e-12);
    }

    /// 6.3 drift gate: the committed `analysis/calibration_export.json` must
    /// exactly match what `calibration_export_json()` produces right now. Any
    /// edit to a constant that isn't mirrored in the export fails the release
    /// gate. Regenerate with `cargo test -p avian_core -- --nocapture` after
    /// re-exporting (a dedicated `--export-calibration` step writes the file).
    #[test]
    fn calibration_export_matches_committed_json() {
        let current = calibration_export_json();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../analysis/calibration_export.json"
        );
        let committed = std::fs::read_to_string(path)
            .expect("analysis/calibration_export.json missing — regenerate it");
        let committed_json: serde_json::Value =
            serde_json::from_str(&committed).expect("committed export is not valid JSON");
        assert_eq!(
            current, committed_json,
            "calibration_export.json drifted from calibration.rs — re-run the calibration export step"
        );
    }
}
