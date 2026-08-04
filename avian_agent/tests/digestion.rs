//! Audit 5a (Sprint 1): the crop→gizzard digestion pipeline must actually run.
//!
//! Root cause: `metabolism_system` used `(0.1 * dt) as u32` to move grain from
//! the crop to the gizzard. At the fixed dt = 1/120 s, `0.1 * (1/120) = 0.00083`
//! truncates to 0, so no grain ever left the crop. The crop stayed full, hunger
//! (computed only from crop/gizzard fullness) stayed pinned at ~0.28, and birds
//! never foraged again after their first meal.
//!
//! Second deadlock: the gizzard is a one-way buffer capped at
//! `GIZZARD_CAPACITY_GRANS`. Even with a working crop→gizzard transfer it filled
//! to the cap and blocked the transfer forever. The gizzard now also drains back
//! into the bloodstream, so a full crop can empty over time.
//!
//! These tests assert the pipeline flows at dt = 1/120 (the real fixed step) and
//! that a bird actually gets hungry again after its crop drains.

use avian_agent::metabolism::metabolism_system;
use avian_agent::systems::run_systems;
use avian_core::calibration;
use avian_core::components::{FSMState, Mass, Metabolism, Position, Velocity};
use avian_core::time::SimulationTime;
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use hecs::World;
use nalgebra::Vector2;

/// Build a bare single-agent world (no physics) for testing `metabolism_system`
/// in isolation. Mimics the spawn defaults from `gerontology::spawn_agent`.
fn bare_world() -> (World, hecs::Entity) {
    let mut world = World::new();
    let e = world.spawn((
        Position(Vector2::new(16.0, 10.5)),
        Velocity(Vector2::zeros()),
        Mass {
            base_g: 315.0,
            condition_factor: 0.0,
            current_g: 315.0,
        },
        Metabolism {
            bmr_watts: calibration::bmr_for_mass(315.0),
            energy_kj: 50.0,
            hunger: 0.2,
            crop_count: 8,
            gizzard_count: 3,
            crop_max: 32,
            last_peck_time: 0.0,
            digest_carry_s: 0.0,
            gizzard_drain_carry_s: 0.0,
        },
        FSMState::Spacer,
    ));
    (world, e)
}

/// A bird with `crop_count = 5`, `gizzard_count = 3`. Run `metabolism_system` at
/// the real fixed dt = 1/120 and assert the crop drains over ~50 sim-seconds
/// (0.1 grain/s → 5 grains in 50 s) instead of hanging at the truncated
/// 0-transfer forever.
#[test]
fn crop_drains_over_biological_timescale_at_real_dt() {
    let (mut world, e) = bare_world();
    world.get::<&mut Metabolism>(e).unwrap().crop_count = 5;

    let dt = 1.0 / 120.0;
    let mut time = SimulationTime::new(dt);
    let mut total_digested = 0.0;
    let mut digested_frames = 0u32;

    // 60 sim-seconds at dt=1/120 → 7200 ticks.
    for _ in 0..(60 * 120) {
        time.tick();
        let (_drained, digested) = metabolism_system(&mut world, &time, 1.0, 1.0, 1.0);
        total_digested += digested;
        if digested > 0.0 {
            digested_frames += 1;
        }
    }

    let meta = world.get::<&Metabolism>(e).unwrap();
    assert_eq!(
        meta.crop_count, 0,
        "crop must drain to 0 within 60 sim-sec (was {} grains left)",
        meta.crop_count
    );
    assert!(
        total_digested > 0.0,
        "no digestion inflow was ever produced"
    );
    assert!(
        digested_frames > 0,
        "digestion never produced a non-zero inflow frame"
    );
    // 5 grains × (0.5 kJ − 10% TEF) = 5 × 0.45 = 2.25 kJ.
    assert!(
        (total_digested - 5.0 * calibration::GRAIN_ENERGY_KJ * 0.9).abs() < 1e-9,
        "digestion inflow {} kJ != expected 2.25 kJ",
        total_digested
    );
}

/// The gizzard deadlock: with a full crop, the gizzard (capped at 10) must still
/// let the crop drain because the gizzard itself drains back to the blood.
#[test]
fn full_crop_empties_even_with_gizzard_at_cap() {
    let (mut world, e) = bare_world();
    {
        let mut meta = world.get::<&mut Metabolism>(e).unwrap();
        meta.crop_count = 20;
        meta.gizzard_count = calibration::GIZZARD_CAPACITY_GRANS;
    }

    let dt = 1.0 / 120.0;
    let mut time = SimulationTime::new(dt);
    // 20 grains at 0.1 grain/s → 200 s; run 300 s to be safe.
    for _ in 0..(300 * 120) {
        time.tick();
        metabolism_system(&mut world, &time, 1.0, 1.0, 1.0);
    }

    let meta = world.get::<&Metabolism>(e).unwrap();
    assert_eq!(
        meta.crop_count, 0,
        "a crop starting at 20 grains must fully drain even with a full gizzard \
         (left {})",
        meta.crop_count
    );
}

/// End-to-end: a bird eats, its crop drains, and hunger rises back above the
/// foraging threshold so it seeks seed again. Regression for "pigeons are never
/// hungry" as observed live.
#[test]
fn bird_becomes_hungry_again_after_its_crop_drains() {
    let mut sim = Simulation::new(11, SimulationConfig::default());
    let uid = sim.next_uid_str();
    let e = avian_agent::gerontology::spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        Vector2::new(16.0, 10.5),
        &mut sim.physics,
        uid,
    );
    // Young, healthy bird (no Sick shuffle noise) with a modest crop so the
    // hunger cycle completes in a short test window.
    {
        let mut meta = sim.world.get::<&mut Metabolism>(e).unwrap();
        meta.crop_count = 8;
        meta.gizzard_count = 3;
        meta.hunger = 0.2;
        meta.energy_kj = 45.0;
    }
    let mut exporter = TelemetryExporter::new(usize::MAX);

    let mut became_hungry = false;
    for _ in 0..(5 * 60 * 120) {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        let meta = sim.world.get::<&Metabolism>(e).unwrap();
        if meta.hunger >= calibration::FORAGING_HUNGER_THRESHOLD {
            became_hungry = true;
            break;
        }
    }

    assert!(
        became_hungry,
        "bird never reached the foraging hunger threshold {} after its crop \
         drained (digestion must keep running)",
        calibration::FORAGING_HUNGER_THRESHOLD
    );
}
