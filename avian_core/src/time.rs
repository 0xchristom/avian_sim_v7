use serde::{Deserialize, Serialize};

/// Sprint 1 (Audit 5): the simulation clock is a **fixed-step accumulator**.
/// `dt` is the fixed simulation timestep in seconds (default 1/120) — every
/// simulation system and Rapier advance by exactly this delta per `tick`.
///
/// Contract:
/// - `dt` must be finite and > 0. A zero/NaN/infinite step can never advance
///   the clock and must be rejected at construction (see `SimulationConfig::validate`).
/// - `tick()` adds exactly `dt` of sim-time to the accumulator. The caller
///   (a fixed-rate loop or a headless frame loop) must NOT pass an arbitrary
///   wall-clock delta here — that is a different (transport) concern and has
///   its own explicit API in the server.
/// - `consume_tick()` drains one `dt` step at a time, so a frame that was late
///   on the wall clock still steps the simulation in whole fixed increments
///   (never a fractional, wall-clock-dependent step).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SimulationTime {
    pub dt: f64,
    pub frame: u32,
    pub time_us: u64,
    pub accumulator: f64,
    /// Sprint 5 (B27): sub-microsecond remainder carried forward between steps
    /// so `time_us` never drifts from `frame * dt`. `dt * 1e6` is not a whole
    /// number (e.g. 1/120 s → 8333.33 µs), so a naive `as u64` truncation loses
    /// ~0.33 µs/tick — 3.3 ms after 10,000 steps. This field keeps the exact
    /// fractional remainder instead.
    frac_us: f64,
}

impl SimulationTime {
    /// Build a fixed-step clock. `dt` must be finite and > 0 — the caller is
    /// responsible for validating it (config validation guarantees this).
    pub fn new(dt: f64) -> Self {
        debug_assert!(
            dt.is_finite() && dt > 0.0,
            "SimulationTime::new: dt must be finite and > 0"
        );
        Self {
            dt,
            frame: 0,
            time_us: 0,
            accumulator: 0.0,
            frac_us: 0.0,
        }
    }

    /// Add one fixed `dt` of sim-time to the accumulator. This is the ONLY way
    /// sim-time enters the clock. A caller needing wall-clock pacing uses the
    /// server's broadcast interval — never a fractional `tick` delta.
    pub fn tick(&mut self) {
        debug_assert!(self.dt.is_finite() && self.dt > 0.0);
        self.accumulator += self.dt;
    }

    /// Consume exactly one fixed step if one is due. Returns `false` when the
    /// accumulated sim-time is less than one full `dt` (nothing to advance).
    /// Advances `time_us` by exactly `dt` microseconds with a fractional
    /// remainder (B27) so long runs never accumulate truncation drift.
    pub fn consume_tick(&mut self) -> bool {
        if self.accumulator >= self.dt {
            self.accumulator -= self.dt;
            self.frac_us += self.dt * 1_000_000.0;
            let whole = self.frac_us.floor();
            self.frac_us -= whole;
            self.time_us += whole as u64;
            true
        } else {
            false
        }
    }

    /// Sprint 1 (Audit 5): a fractional overflow beyond a whole number of `dt`
    /// steps. A fixed-step loop keeps this as `accumulator`; callers that need
    /// to report sub-step progress (e.g. interpolation) can read it without
    /// ever feeding it back into the clock.
    pub fn step_fraction(&self) -> f64 {
        if self.dt <= 0.0 {
            return 0.0;
        }
        self.accumulator / self.dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_step_accumulates_whole_steps() {
        let mut t = SimulationTime::new(1.0 / 120.0);
        // 1/120 s of sim-time per tick; after one tick one step is due.
        t.tick();
        assert!(t.consume_tick());
        assert!(!t.consume_tick(), "no second step from a single tick");
        assert_eq!(
            t.frame, 0,
            "frame is incremented by the loop, not consume_tick"
        );
    }

    #[test]
    fn two_ticks_produce_two_steps() {
        let mut t = SimulationTime::new(0.5);
        t.tick();
        t.tick();
        assert!(t.consume_tick());
        assert!(t.consume_tick());
        assert!(!t.consume_tick());
        assert!(t.accumulator.abs() < 1e-12, "accumulator fully drained");
    }

    #[test]
    fn step_fraction_reports_overflow_without_feedback() {
        let mut t = SimulationTime::new(1.0 / 120.0);
        t.tick();
        let frac = t.step_fraction();
        assert!((frac - 1.0).abs() < 1e-12);
        assert!(
            t.accumulator > 0.0,
            "reading the fraction never drains the clock"
        );
    }
}
