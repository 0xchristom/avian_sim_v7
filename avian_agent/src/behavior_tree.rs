//! Behavior tree (2.0 Decision Architecture).
//!
//! # Single authoritative root Selector priority
//!
//! ```text
//! root = Selector [
//!     Flee,              // 2.2 — highest priority, overrides ALL
//!     Sick,              // 2.7 — debility; a sick bird still evades a predator
//!     CriticalEnergy,    // 2.0 — energy < threshold → force forage
//!     NightRest,         // 2.3 — light < 0.3 → rest (stop moving, reduced drain)
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

use avian_core::components::*;
use avian_core::calibration;
use avian_core::rng::SimRng;
use nalgebra::Vector2;
use hecs::Entity;
use crate::search::{levy_step, crw_direction};

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
}

#[derive(Debug)]
pub enum BTStatus { Success, Failure, Running }

pub trait BTNode { fn tick(&self, ctx: &mut AgentContext) -> BTStatus; }

pub struct Sequence { children: Vec<Box<dyn BTNode>> }
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

pub struct Selector { children: Vec<Box<dyn BTNode>> }
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
impl BTNode for Action { fn tick(&self, ctx: &mut AgentContext) -> BTStatus { (self.0)(ctx) } }

pub struct Condition(pub fn(&AgentContext) -> bool);
impl BTNode for Condition {
    fn tick(&self, ctx: &mut AgentContext) -> BTStatus {
        if (self.0)(ctx) { BTStatus::Success } else { BTStatus::Failure }
    }
}

// ---- 2.2 Flee -------------------------------------------------------------

fn flee_action(ctx: &mut AgentContext) -> BTStatus {
    if !ctx.fleeing { return BTStatus::Failure; }
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
    // Falls through to the next child if no food is known.
    if ctx.grains.is_empty() { return BTStatus::Failure; }
    *ctx.fsm = FSMState::Foraging;
    forage_move(ctx)
}

// ---- 2.3 NightRest --------------------------------------------------------

fn night_rest_condition(ctx: &AgentContext) -> bool {
    ctx.light_level < calibration::NIGHT_REST_LIGHT_THRESHOLD
}

fn night_rest_action(ctx: &mut AgentContext) -> BTStatus {
    // Stop moving; energy drain reduction is applied in metabolism_system.
    *ctx.fsm = FSMState::Idle;
    ctx.vel.0 = Vector2::zeros();
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
    ctx.feathers.0 = (ctx.feathers.0 + calibration::FEATHER_PREEN_RESTORE_RATE_S * ctx.dt)
        .min(1.0);
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

fn forage_move(ctx: &mut AgentContext) -> BTStatus {
    let (_, g_pos, _) = ctx.grains[0];
    let dist = (g_pos - ctx.pos.0).norm();

    if dist < 0.5 {
        ctx.vel.0 = Vector2::zeros();
    } else {
        let dir = (g_pos - ctx.pos.0).normalize();
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
    if ctx.meta.hunger < calibration::FORAGING_HUNGER_THRESHOLD || ctx.grains.is_empty() {
        return BTStatus::Failure;
    }
    *ctx.fsm = FSMState::Foraging;
    forage_move(ctx)
}

// ---- Wander ---------------------------------------------------------------

fn spacer_action(ctx: &mut AgentContext) -> BTStatus {
    *ctx.fsm = FSMState::Spacer;
    let speed = ctx.mobility.max_speed_ms * 0.8;
    
    if ctx.levy.remaining_dist <= 0.0 {
        let dist = levy_step(ctx.rng, 2.0).min(5.0);
        ctx.levy.target_heading = crw_direction(ctx.head.0, ctx.rng, 2.0);
        ctx.levy.remaining_dist = dist;
    } else {
        ctx.levy.remaining_dist -= speed * ctx.dt;
    }
    
    let mut diff = ctx.levy.target_heading - ctx.head.0;
    while diff > std::f64::consts::PI { diff -= std::f64::consts::TAU; }
    while diff < -std::f64::consts::PI { diff += std::f64::consts::TAU; }
    let turn_rate = ctx.mobility.max_angular_speed_rads * ctx.dt;
    ctx.head.0 += diff.clamp(-turn_rate, turn_rate);
    
    ctx.vel.0 = Vector2::new(speed * ctx.head.0.cos(), speed * ctx.head.0.sin());
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
            // 2.3 NightRest — light < 0.3 → rest.
            Box::new(Sequence {
                children: vec![
                    Box::new(Condition(night_rest_condition)),
                    Box::new(Action(night_rest_action)),
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
