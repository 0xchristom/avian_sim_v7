//! 4.4 Weather tests: stochastic scheduler with smooth ramps, event-forced
//! weather, rain cutting vision range (a bird stops foraging for distant
//! grain), heat raising energy expenditure, and wind drifting the flock.

use avian_agent::systems::run_systems;
use avian_core::calibration;
use avian_core::components::{Age, FSMState, Metabolism, Weather};
use avian_core::events::{Event, SetWeatherRequest};
use avian_core::{Simulation, SimulationConfig};
use avian_telemetry::exporter::TelemetryExporter;
use nalgebra::Vector2;

/// A single quiet bird, no grain, no immigration — the shared harness for
/// drain/drift tests. `energy` is pinned so the bird never starves.
fn single_bird(seed: u64) -> Simulation {
    let config = SimulationConfig {
        immigration_enabled: false,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(seed, config);
    let uid = sim.next_uid_str();
    let e = avian_agent::gerontology::spawn_agent(
        &mut sim.world,
        &mut sim.rng,
        Vector2::new(5.0, 5.0),
        &mut sim.physics,
        uid,
    );
    let mut meta = sim.world.get::<&mut Metabolism>(e).unwrap();
    meta.energy_kj = 50.0;
    meta.crop_count = 0;
    drop(meta);
    sim.world.get::<&mut Age>(e).unwrap().years = 1.0;
    sim
}

/// Rain hides grain that would be visible in clear weather: a starving bird
/// force-forages for a 9 m patch in clear skies (within 10 m vision) but can
/// never see it in rain (vision cut to 6 m), so it falls through to wandering.
#[test]
fn rain_reduces_vision_and_stops_foraging_for_distant_grain() {
    let fsm_forage_ratio = |weather: Weather, intensity: f64| -> f64 {
        let mut sim = single_bird(1234);
        sim.environment.weather = weather;
        sim.environment.weather_intensity = intensity;
        // Pin critical energy so the root tree force-forages.
        let e = sim.world.query::<&Metabolism>().iter().next().unwrap().0;
        let mut meta = sim.world.get::<&mut Metabolism>(e).unwrap();
        meta.energy_kj = 4.0;
        drop(meta);
        spawn_grain_at(&mut sim, Vector2::new(14.0, 5.0)); // 9 m east of the bird

        let mut exporter = TelemetryExporter::new(usize::MAX);
        let mut forage = 0u32;
        let mut total = 0u32;
        for _ in 0..120 {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
            for (_, fsm) in sim.world.query::<&FSMState>().iter() {
                if *fsm == FSMState::Foraging {
                    forage += 1;
                }
                total += 1;
            }
        }
        forage as f64 / total as f64
    };

    let clear = fsm_forage_ratio(Weather::Clear, 0.0);
    let rain = fsm_forage_ratio(Weather::Rain, 1.0);

    assert!(
        clear > 0.9,
        "clear skies: starving bird should forage almost every frame (got {clear:.2})"
    );
    assert!(
        rain < 0.05,
        "rain: bird must NOT see the 9 m grain (vision 6 m), got {rain:.2} foraging"
    );
    assert!(
        clear > rain,
        "clear foraging ({clear:.2}) must beat rain ({rain:.2})"
    );
}

fn spawn_grain_at(sim: &mut Simulation, pos: Vector2<f64>) {
    avian_agent::systems::spawn_grain(sim, pos, 100);
}

/// Heat multiplies basal metabolism → strictly higher energy expenditure than
/// the same run in clear weather (same seed → identical movement, only the
/// BMR multiplier differs).
#[test]
fn heat_increases_energy_expenditure() {
    let expenditure = |weather: Weather, intensity: f64| -> f64 {
        let mut sim = single_bird(7);
        sim.environment.weather = weather;
        sim.environment.weather_intensity = intensity;
        let mut exporter = TelemetryExporter::new(usize::MAX);
        for _ in 0..600 {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        }
        sim.total_energy_expenditure_kj
    };

    let clear = expenditure(Weather::Clear, 0.0);
    let heat = expenditure(Weather::Heat, 1.0);
    assert!(
        heat > clear * 1.03,
        "heat should raise energy burn: heat={heat:.4} kJ vs clear={clear:.4} kJ"
    );
}

/// Wind drifts every body: with an easterly wind the lone bird's x-displacement
/// must clearly exceed the no-wind control (same seed).
#[test]
fn wind_drifts_agents() {
    let x_displacement = |weather: Weather, intensity: f64| -> f64 {
        let mut sim = single_bird(99);
        sim.environment.weather = weather;
        sim.environment.weather_intensity = intensity;
        sim.environment.wind_heading = 0.0; // easterly
        let start = bird_x(&sim);
        let mut exporter = TelemetryExporter::new(usize::MAX);
        for _ in 0..300 {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        }
        bird_x(&sim) - start
    };

    let clear_dx = x_displacement(Weather::Clear, 0.0);
    let wind_dx = x_displacement(Weather::Wind, 1.0);
    assert!(
        wind_dx > clear_dx + 4.0,
        "wind should blow the bird east: wind dx={wind_dx:.2} vs clear dx={clear_dx:.2}"
    );
}

fn bird_x(sim: &Simulation) -> f64 {
    sim.world
        .query::<&avian_core::components::Position>()
        .iter()
        .next()
        .map(|(_, p)| p.0.x)
        .unwrap_or(0.0)
}

/// `SetWeather` events apply immediately and the intensity ramps smoothly to
/// the target over ~1 sim-second, then back down when cleared.
#[test]
fn set_weather_event_applies_and_ramps_smoothly() {
    let mut sim = single_bird(1);
    // The smooth intensity ramp runs inside the (config-gated) scheduler.
    sim.config.weather_enabled = true;

    let mut exporter = TelemetryExporter::new(usize::MAX);
    sim.inject_event(Event::SetWeather(SetWeatherRequest {
        weather: Weather::Rain,
    }));
    for _ in 0..130 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    assert_eq!(sim.environment.weather, Weather::Rain);
    assert!(
        sim.environment.weather_intensity > 0.9,
        "rain intensity should ramp to ~1.0, got {}",
        sim.environment.weather_intensity
    );

    sim.inject_event(Event::SetWeather(SetWeatherRequest {
        weather: Weather::Clear,
    }));
    for _ in 0..130 {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
    }
    assert_eq!(sim.environment.weather, Weather::Clear);
    assert!(
        sim.environment.weather_intensity < 0.1,
        "rain intensity should ramp back to 0, got {}",
        sim.environment.weather_intensity
    );
}

/// The snapshot surfaces the current weather for the viewer.
#[test]
fn snapshot_includes_weather() {
    let mut sim = single_bird(2);
    sim.environment.weather = Weather::Wind;
    sim.environment.weather_intensity = 0.5;
    let snap = sim.snapshot();
    assert_eq!(snap.weather, Weather::Wind);
    assert_eq!(snap.weather_intensity, 0.5);
}

/// The scheduler actually leaves Clear over time (config-gated) and keeps the
/// intensity bounded — a sanity pass over a full re-roll cycle.
#[test]
fn scheduler_eventually_changes_weather() {
    let config = SimulationConfig {
        weather_enabled: true,
        ..SimulationConfig::default()
    };
    let mut sim = Simulation::new(2024, config);
    let mut exporter = TelemetryExporter::new(usize::MAX);
    let mut non_clear = 0u32;
    for _ in 0..(calibration::WEATHER_UPDATE_INTERVAL_FRAMES * 3) {
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        assert!((0.0..=1.0).contains(&sim.environment.weather_intensity));
        if sim.environment.weather != Weather::Clear {
            non_clear += 1;
        }
    }
    assert!(non_clear > 0, "scheduler never left Clear for seed 2024");
}
