//! 4.4 Weather scheduler — stochastic global weather with smooth transitions.
//!
//! The scheduler re-rolls the weather state (Clear / Rain / Wind / Heat) every
//! `WEATHER_UPDATE_INTERVAL_FRAMES` and smoothly ramps `weather_intensity`
//! toward 1 (non-Clear) or 0 (Clear) at `WEATHER_RAMP_RATE_PER_S`, so effects
//! fade in/out over ~1 sim-second instead of snapping. All draws come from the
//! shared `sim.rng` at a fixed point in the frame, keeping runs deterministic.
//!
//! Gated by `SimulationConfig::weather_enabled` — with the flag off the
//! scheduler never touches the RNG stream and weather stays `Clear`, so the
//! existing deterministic scenarios keep their exact trajectories.

use avian_core::calibration;
use avian_core::components::Weather;
use avian_core::Simulation;

/// Step the global weather forward one frame. No-op unless weather is enabled.
pub fn update(sim: &mut Simulation) {
    if !sim.config.weather_enabled {
        return;
    }

    // Smooth intensity ramp toward the active weather.
    let target = if sim.environment.weather == Weather::Clear {
        0.0
    } else {
        1.0
    };
    let step = calibration::WEATHER_RAMP_RATE_PER_S * sim.config.dt;
    let i = sim.environment.weather_intensity;
    let next = if i < target {
        (i + step).min(target)
    } else {
        (i - step).max(target)
    };
    sim.environment.weather_intensity = next;

    // Re-roll the state on the fixed cadence.
    if sim.environment.weather_frames_left == 0 {
        sim.environment.weather_frames_left = calibration::WEATHER_UPDATE_INTERVAL_FRAMES;
        let new = match sim.rng.gen_range(0..4) {
            0 => Weather::Clear,
            1 => Weather::Rain,
            2 => Weather::Wind,
            _ => Weather::Heat,
        };
        sim.environment.weather = new;
        if new == Weather::Wind {
            sim.environment.wind_heading = sim.rng.gen_range(0.0..std::f64::consts::TAU);
        }
    } else {
        sim.environment.weather_frames_left -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian_core::SimulationConfig;

    #[test]
    fn disabled_weather_never_changes() {
        let mut sim = Simulation::new(1, SimulationConfig::default());
        let initial = sim.environment.weather;
        for _ in 0..(calibration::WEATHER_UPDATE_INTERVAL_FRAMES * 3 + 50) {
            sim.step(|s, _| update(s));
        }
        assert_eq!(sim.environment.weather, initial);
        assert_eq!(sim.environment.weather_intensity, 0.0);
    }

    #[test]
    fn enabled_weather_stays_bounded_and_ramps_smoothly() {
        let config = SimulationConfig {
            weather_enabled: true,
            ..SimulationConfig::default()
        };
        let mut sim = Simulation::new(7, config);
        let mut max_intensity = 0.0f64;
        for _ in 0..(calibration::WEATHER_UPDATE_INTERVAL_FRAMES * 5) {
            sim.step(|s, _| update(s));
            assert!((0.0..=1.0).contains(&sim.environment.weather_intensity));
            max_intensity = max_intensity.max(sim.environment.weather_intensity);
        }
        // With a 4-state scheduler over 5 re-rolls, at least one non-Clear
        // state should have fired for this seed and ramped intensity up.
        assert!(max_intensity > 0.5, "weather never left Clear for seed 7");
    }
}
