//! Behavior tree (2.0 Decision Architecture).
//!
//! # Single authoritative root Selector priority
//!
//! ```text
//! root = Selector [
//!     Flee,              // 2.2 — highest priority, overrides ALL
//!     Sick,              // 2.7 — debility; a sick bird still evades a predator
//!     CriticalEnergy,    // 2.0 — energy < threshold → force forage
//!     Roosting,          // Audit 4 §9.5 — light < 0.3 → sleep; ~12% sentinels
//!                        //                stay in Scanning instead (drawn from
//!                        //                sim.rng each tick, never same birds)
//!     Glide,             // Phase 9 — airborne in a building thermal, aligned with updraft
//!     Preen,             // 2.6 — feathers_condition < threshold
//!     Forage,            // existing — hunger > 0.4 (memory-biased target, 4.2)
//!     Wander             // existing — Levy/CRW spacer
//! ]
//! ```
//!
//! This is THE priority list. Any future behavior is inserted into THIS list,
//! never added as a sibling system (code-review enforced).
//!
//! # Composition rules
//!
//! - **Boids is a steering force, NOT a tree branch** (2.1): flocking computes
//!   a steering vector that is *summed onto whatever velocity/heading the tree
//!   already selected* — exactly like head-bob overlays locomotion. The tree
//!   decides the *goal*; boids only modulates *how* the pigeon moves through
//!   its flock. Fleeing is the exception: while Flee is active, boids steering
//!   is suppressed entirely.
//! - **Memory-biased search feeds the existing Forage condition, not a new
//!   condition node** (4.2): when a grain target is needed, `foraging_action`
//!   picks from visible grains, else from memory slots weighted by strength.
//!   If NO target exists (neither visible nor remembered), `foraging_action`
//!   returns `Failure`, so the root Selector falls through to `Wander`. There
//!   is NO "stand still" fallback — the one condition is exactly:
//!   "hungry AND (grain visible OR remembered food exists); otherwise Wander".

use crate::search::{crw_direction, levy_step};
use avian_core::calibration;
use avian_core::components::*;
use avian_core::rng::SimRng;
use hecs::Entity;
use nalgebra::Vector2;

pub struct AgentContext<'a> {
    pub pos: &'a mut Position,
    pub head: &'a mut Heading,
    pub vel: &'a mut Velocity,
    pub meta: &'a mut Metabolism,
    pub fsm: &'a mut FSMState,
    pub levy: &'a mut LevyState,
    pub mass: &'a Mass,
    pub mobility: &'a Mobility,
    pub vision: &'a Vision,
    pub head_bob: &'a mut HeadBob,
    pub neighbors: Vec<Entity>, // Uproszczono typ
    pub grains: Vec<(Entity, Vector2<f64>, u32)>,
    pub rng: &'a mut SimRng,
    pub dt: f64,
    // 2.0/2.3 environment inputs
    pub light_level: f64,
    // 2.6 feather condition (decay applied in run_systems; restored here).
    pub feathers: &'a mut FeatherCondition,
    // 2.2 threat inputs (set by predator::collect_threats before tick)
    pub fleeing: bool,
    pub flee_dir: Vector2<f64>,
    // 2.7 debility flag (vitality < SICK_VITALITY_THRESHOLD)
    pub sick: bool,
    // 4.2 spatial memory: best remembered food location (weighted by strength),
    // computed in run_systems before the tick. Used only when no grain is
    // visible (memory-biased search feeds the existing Forage condition).
    pub memory_target: Option<Vector2<f64>>,
    // 5.2: scenario-tunable hunger threshold for the root Forage condition.
    // `run_systems` fills it from `SimulationConfig::foraging_threshold`
    // (default = the biology constant FORAGING_HUNGER_THRESHOLD).
    pub forage_hunger_threshold: f64,
    // Phase 9 (Audit 3): building-thermal updraft zones (from `sim.thermals`).
    pub thermals: &'a [ThermalZone],
    // Audit 5a (Sprint 3): arena size (m), so wanderers can steer away from the
    // edges instead of clinging to a wall. Filled from `sim.config`.
    pub world_dims: Vector2<f64>,
}

/// Audit 5a (Sprint 3): blend `from` toward `target` by fraction `t` of the
/// shortest angular gap, staying in `[-π, π]`. Used to aim a wanderer's Lévy
/// step at the arena interior instead of a wall.
fn blend_angle_toward(from: f64, target: f64, t: f64) -> f64 {
    let mut diff = target - from;
    while diff > std::f64::consts::PI {
        diff -= std::f64::consts::TAU;
    }
    while diff < -std::f64::consts::PI {
        diff += std::f64::consts::TAU;
    }
    from + diff * t
}

#[derive(Debug)]
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

pub struct Condition(pub fn(&AgentContext) -> bool);
impl BTNode for Condition {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus {
        if (self.0)(ctx) {
            BTStatus::Success
        } else {
            BTStatus::Failure
        }
    }
}

// ---- 2.2 Flee -------------------------------------------------------------

fn flee_action(ctx: &mut AgentContext) -> BTStatus {
    if !ctx.fleeing {
        return BTStatus::Failure;
    }
    *ctx.fsm = FSMState::Fleeing;
    // 4.1 flight: pigeons flee by FLYING at FLY_SPEED_MS, not a ground sprint
    // (v1 2.2 was an explicit "ground sprint at max_speed_ms" placeholder).
    // The flight metabolic cost is paid via FLIGHT_MR_MULTIPLIER (metabolism).
    let dir = ctx.flee_dir;
    if dir.norm() > 1e-6 {
        let speed = calibration::FLY_SPEED_MS;
        ctx.head.0 = dir.y.atan2(dir.x);
        ctx.vel.0 = dir * speed;
    } else {
        ctx.vel.0 = Vector2::zeros();
    }
    BTStatus::Running
}

// ---- 2.0 CriticalEnergy ---------------------------------------------------

fn critical_energy_condition(ctx: &AgentContext) -> bool {
    ctx.meta.energy_kj < calibration::CRITICAL_ENERGY_THRESHOLD_KJ
}

fn force_forage_action(ctx: &mut AgentContext) -> BTStatus {
    // Force forage even when hunger is low; ignore other non-flee goals.
    // Falls through to the next child if no food is known (visible or
    // remembered, 4.2).
    match pick_forage_target(ctx) {
        Some(target) => {
            *ctx.fsm = FSMState::Foraging;
            forage_move_to(ctx, target)
        }
        None => BTStatus::Failure,
    }
}

// ---- Audit 4 §9.5 Roosting (night sleep + flock sentinels) ----------------

/// Audit 4 §9.5: night eligibility — same light gate as the old NightRest
/// branch (NIGHT_REST_LIGHT_THRESHOLD), which also drives the night energy
/// drain factor in metabolism_system. Roosting sits BELOW CriticalEnergy so a
/// genuinely starving bird still overrides sleep to forage.
fn roosting_condition(ctx: &AgentContext) -> bool {
    ctx.light_level < calibration::NIGHT_REST_LIGHT_THRESHOLD
}

fn roosting_action(ctx: &mut AgentContext) -> BTStatus {
    // Sentinel draw: per-tick, per-eligible-agent, from the shared sim.rng.
    // Deterministic for a fixed seed but NOT tied to agent identity — the same
    // birds never stand guard every night. No new perception machinery: a
    // sentinel uses the existing vision cone / cone_cast that already runs for
    // every agent every tick.
    let sentinel = ctx.rng.gen_range(0.0..1.0) < calibration::SENTINEL_FRACTION;
    if sentinel {
        // Exempted from the roosting override: stay alert and patrol slowly
        // (Levy/CRW heading drift at a fraction of max speed).
        *ctx.fsm = FSMState::Scanning;
        let speed = ctx.mobility.max_speed_ms * calibration::SENTINEL_PATROL_SPEED_FRACTION;
        if ctx.levy.remaining_dist <= 0.0 {
            let dist = levy_step(ctx.rng, 2.0).min(5.0);
            ctx.levy.target_heading = crw_direction(ctx.head.0, ctx.rng, 2.0);
            ctx.levy.remaining_dist = dist;
        } else {
            ctx.levy.remaining_dist -= speed * ctx.dt;
        }
        let mut diff = ctx.levy.target_heading - ctx.head.0;
        while diff > std::f64::consts::PI {
            diff -= std::f64::consts::TAU;
        }
        while diff < -std::f64::consts::PI {
            diff += std::f64::consts::TAU;
        }
        let turn_rate = ctx.mobility.max_angular_speed_rads * ctx.dt;
        ctx.head.0 += diff.clamp(-turn_rate, turn_rate);
        ctx.vel.0 = Vector2::new(speed * ctx.head.0.cos(), speed * ctx.head.0.sin());
    } else {
        // Roost: sleep in place (like Idle, but a visually/semantically
        // distinct label). The reduced night drain is applied automatically via
        // the light gate in metabolism_system.
        *ctx.fsm = FSMState::Roosting;
        ctx.vel.0 = Vector2::zeros();
    }
    BTStatus::Running
}

// ---- 2.6 Preen -------------------------------------------------------------

fn preen_condition(ctx: &AgentContext) -> bool {
    // Hysteresis: enter when feathers fall below the low threshold, keep
    // preening until restored to PREEN_STOP_THRESHOLD (no flicker at 0.3).
    if *ctx.fsm == FSMState::Preening {
        ctx.feathers.0 < calibration::PREEN_STOP_THRESHOLD
    } else {
        ctx.feathers.0 < calibration::PREEN_FEATHER_THRESHOLD
    }
}

fn preen_action(ctx: &mut AgentContext) -> BTStatus {
    *ctx.fsm = FSMState::Preening;
    // Preening pigeons stand still; feathers are restored while doing so.
    ctx.vel.0 = Vector2::zeros();
    ctx.feathers.0 = (ctx.feathers.0 + calibration::FEATHER_PREEN_RESTORE_RATE_S * ctx.dt).min(1.0);
    BTStatus::Running
}

// ---- 2.7 Sick --------------------------------------------------------------

fn sick_condition(ctx: &AgentContext) -> bool {
    ctx.sick
}
fn sick_action(ctx: &mut AgentContext) -> BTStatus {
    // Debilitated: impaired slow forage toward the nearest visible grain, else
    // a slow shuffle. The 50% velocity cut is applied in run_systems so it
    // also slows fleeing (→ more vulnerable to predators). This branch sits
    // right after Flee so a sick bird still evades when a hawk is on it.
    *ctx.fsm = FSMState::Sick;
    if let Some((_, g_pos, _)) = ctx.grains.first() {
        let dist = (*g_pos - ctx.pos.0).norm();
        if dist > 0.5 {
            let dir = (*g_pos - ctx.pos.0).normalize();
            ctx.head.0 = dir.y.atan2(dir.x);
            ctx.vel.0 = dir * ctx.mobility.max_speed_ms;
        } else {
            ctx.vel.0 = Vector2::zeros();
        }
    } else {
        ctx.vel.0 = Vector2::zeros();
    }
    BTStatus::Running
}

// ---- Forage ---------------------------------------------------------------

/// 4.2 memory-biased target selection: nearest visible grain first, else the
/// strongest remembered food location (computed in run_systems), else None.
/// `None` means the Forage branch fails and the root Selector falls to Wander.
fn pick_forage_target(ctx: &AgentContext) -> Option<Vector2<f64>> {
    // Visible grains: nearest one wins (v1 behavior).
    let mut best: Option<Vector2<f64>> = None;
    let mut best_d = f64::INFINITY;
    for (_, g_pos, _) in &ctx.grains {
        let d = (g_pos - ctx.pos.0).norm();
        if d < best_d {
            best_d = d;
            best = Some(*g_pos);
        }
    }
    if best.is_some() {
        return best;
    }
    // No visible grain → remembered location (4.2).
    ctx.memory_target
}

fn forage_move_to(ctx: &mut AgentContext, target: Vector2<f64>) -> BTStatus {
    let dist = (target - ctx.pos.0).norm();
    if dist < 0.5 {
        ctx.vel.0 = Vector2::zeros();
    } else {
        let dir = (target - ctx.pos.0).normalize();
        ctx.head.0 = dir.y.atan2(dir.x);
        let speed = ctx.mobility.max_speed_ms;
        ctx.vel.0 = Vector2::new(speed * ctx.head.0.cos(), speed * ctx.head.0.sin());
    }
    BTStatus::Running
}

fn foraging_action(ctx: &mut AgentContext) -> BTStatus {
    // 2.0c: condition is exactly "hungry AND (grain visible OR remembered food
    // exists)". 4.2 plugs memory-biased target selection in here. With no
    // target, this returns Failure so the root falls through to Wander.
    // 5.2: the hunger threshold is scenario-tunable (config), defaulting to the
    // biology constant — see AgentContext::forage_hunger_threshold.
    if ctx.meta.hunger < ctx.forage_hunger_threshold {
        return BTStatus::Failure;
    }
    match pick_forage_target(ctx) {
        Some(target) => {
            *ctx.fsm = FSMState::Foraging;
            forage_move_to(ctx, target)
        }
        None => BTStatus::Failure,
    }
}

// ---- Wander ---------------------------------------------------------------

fn spacer_action(ctx: &mut AgentContext) -> BTStatus {
    *ctx.fsm = FSMState::Spacer;
    let speed = ctx.mobility.max_speed_ms * 0.8;

    // Audit 5a (Sprint 3): boundary awareness for the wanderer. If we are inside
    // the wall-avoid margin, bias the target heading toward the interior so a
    // CRW/Lévy step never aims a pigeon straight at the wall (which physics then
    // turns into permanent tangential sliding).
    let margin = calibration::WALL_AVOID_MARGIN_M;
    let (w, h) = (ctx.world_dims.x, ctx.world_dims.y);
    let (x, y) = (ctx.pos.0.x, ctx.pos.0.y);
    // Compose a repulsion direction (same shape as the steering term in
    // run_systems) and, if non-trivial, the interior angle to steer toward.
    let mut repel: Vector2<f64> = Vector2::zeros();
    if x < margin {
        repel.x += 1.0;
    } else if x > w - margin {
        repel.x -= 1.0;
    }
    if y < margin {
        repel.y += 1.0;
    } else if y > h - margin {
        repel.y -= 1.0;
    }
    let interior_angle = if repel.norm() < 1e-6 {
        None
    } else {
        Some(repel.y.atan2(repel.x))
    };

    if ctx.levy.remaining_dist <= 0.0 {
        // Audit 4 §9.8: the Lévy step cap is now WANDER_LEVY_MAX_STEP_M (15 m),
        // not the old hardcoded 5 m — the raw heavy-tailed distribution alone
        // still maxed at 5 m, which could never escape a cluster's gravity well.
        let dist = levy_step(ctx.rng, 2.0).min(calibration::WANDER_LEVY_MAX_STEP_M);
        // Audit 5a (Sprint 3): a fresh step near a wall starts aimed at the
        // interior (angle blend toward the repulsion direction) so the agent
        // turns away instead of marching at the wall.
        let base = crw_direction(ctx.head.0, ctx.rng, 2.0);
        ctx.levy.target_heading = match interior_angle {
            Some(t) => blend_angle_toward(base, t, 0.5),
            None => base,
        };
        ctx.levy.remaining_dist = dist;
    } else {
        // Audit 5a (Sprint 3): two guards against edge-clinging on an ACTIVE step.
        // (1) If we are inside the margin pushing toward the wall, steer the
        //     target heading toward the interior instead of burning the step
        //     while stuck — the bird turns away and the step then advances once
        //     its heading no longer faces the wall.
        // (2) Otherwise the step advances normally.
        let heading = Vector2::new(ctx.head.0.cos(), ctx.head.0.sin());
        let mut pointing_outward = false;
        if x < margin {
            pointing_outward |= heading.x < 0.0;
        }
        if x > w - margin {
            pointing_outward |= heading.x > 0.0;
        }
        if y < margin {
            pointing_outward |= heading.y < 0.0;
        }
        if y > h - margin {
            pointing_outward |= heading.y > 0.0;
        }
        if pointing_outward {
            if let Some(t) = interior_angle {
                ctx.levy.target_heading = blend_angle_toward(ctx.levy.target_heading, t, 0.5);
            }
        } else {
            ctx.levy.remaining_dist -= speed * ctx.dt;
        }
    }

    let mut diff = ctx.levy.target_heading - ctx.head.0;
    while diff > std::f64::consts::PI {
        diff -= std::f64::consts::TAU;
    }
    while diff < -std::f64::consts::PI {
        diff += std::f64::consts::TAU;
    }
    let turn_rate = ctx.mobility.max_angular_speed_rads * ctx.dt;
    ctx.head.0 += diff.clamp(-turn_rate, turn_rate);

    ctx.vel.0 = Vector2::new(speed * ctx.head.0.cos(), speed * ctx.head.0.sin());
    BTStatus::Running
}

// ---- Phase 9 Glide (building thermals) -------------------------------------

/// Phase 9 (Audit 3): the Glide condition — the bird is inside a
/// building-thermal updraft zone and its heading aligns with the updraft
/// `flow` vector. The sim has no voluntary flight outside Fleeing (which
/// outranks this branch), so the thermal itself provides the launch: a pigeon
/// walking the sun-facing wall whose heading matches the rising airflow takes
/// off and soars — the updraft carries it aloft. Gliding then sets the
/// airborne speed and collapses MR to near-zero (see `glide_action`).
fn glide_condition(ctx: &AgentContext) -> bool {
    let heading = Vector2::new(ctx.head.0.cos(), ctx.head.0.sin());
    let max_align = calibration::GLIDE_HEADING_ALIGN_DEG.to_radians();
    for t in ctx.thermals {
        if ctx.pos.0.x < t.min.x || ctx.pos.0.x > t.max.x {
            continue;
        }
        if ctx.pos.0.y < t.min.y || ctx.pos.0.y > t.max.y {
            continue;
        }
        let cos_ang = (heading.dot(&t.flow)).clamp(-1.0, 1.0);
        if cos_ang >= max_align.cos() {
            return true;
        }
    }
    false
}

fn glide_action(ctx: &mut AgentContext) -> BTStatus {
    *ctx.fsm = FSMState::Gliding;
    // Soar the updraft: cruise straight along the current heading at
    // GLIDE_SPEED_MS. MR collapses to GLIDE_MR_MULTIPLIER (applied in the
    // drain systems); steering agility is restricted in run_systems.
    ctx.vel.0 = Vector2::new(
        calibration::GLIDE_SPEED_MS * ctx.head.0.cos(),
        calibration::GLIDE_SPEED_MS * ctx.head.0.sin(),
    );
    BTStatus::Running
}

pub fn build_default_tree() -> Box<dyn BTNode> {
    Box::new(Selector {
        children: vec![
            // 2.2 Flee — highest priority, overrides ALL.
            Box::new(Action(flee_action)),
            // 2.7 Sick — debility right after flee; a sick bird still evades.
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(sick_condition)),
                    Box::new(Action(sick_action)),
                ],
            }),
            // 2.0 CriticalEnergy — force forage, ignore other non-flee goals.
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(critical_energy_condition)),
                    Box::new(Action(force_forage_action)),
                ],
            }),
            // Audit 4 §9.5 Roosting — light < 0.3 → sleep in place; a fixed
            // sentinel fraction stays in Scanning instead (drawn per-tick from
            // sim.rng). Replaces the old NightRest branch (which merely set
            // Idle). Below CriticalEnergy: a starving bird overrides sleep.
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(roosting_condition)),
                    Box::new(Action(roosting_action)),
                ],
            }),
            // Phase 9 — Glide: airborne + inside a building thermal + heading
            // aligned with the updraft. Overrides Preen/Forage/Wander (a bird
            // riding a thermal cannot forage or groom), but sits below the
            // survival branches (Flee/Sick/CriticalEnergy/NightRest).
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(glide_condition)),
                    Box::new(Action(glide_action)),
                ],
            }),
            // 2.6 Preen — feathers below threshold.
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(preen_condition)),
                    Box::new(Action(preen_action)),
                ],
            }),
            // Forage — hunger > 0.4 (memory-biased target, 4.2).
            Box::new(Action(foraging_action)),
            // Wander — Levy/CRW spacer.
            Box::new(Action(spacer_action)),
        ],
    })
}
