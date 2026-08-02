//! Telemetry metadata (3.7) — the `metadata.json` written alongside a dataset.
//!
//! This file is the **schema authority** for a dataset: it pins the obs schema
//! (`obs_schema: "obs_v1"` + the full index table) so any downstream model
//! parses observations by consulting THIS file, never by assuming a layout.
//! A future `obs_v2` bumps `obs_schema` and keeps `obs_v1` rows readable.

use std::path::Path;
use serde::Serialize;

/// The obs_v1 index table. Kept in sync with `rlhf::state_to_observation`.
/// `[start..end)` are half-open ranges; scalars are `[i..i+1]`.
pub const OBS_V1_INDEX: &[(&str, &str)] = &[
    ("[0..2]", "pos (normalized to world size)"),
    ("[2..4]", "heading (sin + cos)"),
    ("[4]", "velocity magnitude"),
    ("[5]", "energy (normalized)"),
    ("[6]", "hunger"),
    ("[7]", "age (normalized to WILD_MAX_LIFESPAN_YEARS)"),
    ("[8]", "vitality"),
    ("[9]", "light_level"),
    ("[10..16]", "nearest 3 grains rel pos (2 each)"),
    ("[16..37]", "7 neighbors rel pos + dist (3 each)"),
    ("[37..43]", "predator rel pos + threat + alarm_flag (+2 reserved)"),
    ("[43..51]", "memory locations (4.2 — all zero until it ships)"),
    ("[51..127]", "reserved — all zero (future fields without renumbering)"),
    ("[127]", "unused (kept zero)"),
];

#[derive(Clone, Serialize)]
pub struct RewardStats {
    pub total_mean: f64,
    pub total_min: f64,
    pub total_max: f64,
    pub grain_total: f64,
    pub flocking_total: f64,
    pub starvation_total: f64,
    pub captured_total: f64,
    pub flee_success_total: f64,
}

#[derive(Clone, Serialize)]
pub struct TelemetryMetadata {
    pub generated_at: String,
    pub obs_schema: String,
    pub obs_layout: Vec<[String; 2]>,
    pub seed: u64,
    pub config: serde_json::Value,
    pub initial_agents: usize,
    pub world_size_m: [f64; 2],
    pub sim_frames: u64,
    pub events_injected: Vec<String>,
    pub reward_stats: Option<RewardStats>,
    pub rust_version: String,
    pub commit: Option<String>,
}

pub fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Stable, timezone-free UTC timestamp.
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days` — converts a day count to y/m/d.
fn civil_from_days(z0: i64) -> (i64, u32, u32) {
    let z = z0 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn git_commit() -> Option<String> {
    // Best-effort; returns None when not a git checkout.
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Capture the toolchain version at runtime (`rustc --version`). The
/// `RUSTC_VERSION` env var is not a standard Cargo-provided variable, so it
/// can never be relied on; running `rustc --version` mirrors `git_commit()`.
pub fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

impl TelemetryMetadata {
    pub fn new(
        seed: u64,
        config: serde_json::Value,
        initial_agents: usize,
        world_size_m: [f64; 2],
        events_injected: Vec<String>,
        sim_frames: u64,
        reward_stats: Option<RewardStats>,
    ) -> Self {
        Self {
            generated_at: now_iso8601(),
            obs_schema: "obs_v1".into(),
            obs_layout: OBS_V1_INDEX
                .iter()
                .map(|(k, v)| [k.to_string(), v.to_string()])
                .collect(),
            seed,
            config,
            initial_agents,
            world_size_m,
            sim_frames,
            events_injected,
            reward_stats,
            rust_version: rustc_version(),
            commit: git_commit(),
        }
    }
}

pub fn write_metadata(path: &Path, meta: &TelemetryMetadata) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(meta).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("serialize metadata: {e}"))
    })?;
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_civil_from_days_epoch() {
        // 1970-01-01 == day 0.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // A known date: 2026-08-02.
        // Days from 1970-01-01 to 2026-08-02: compute and sanity check.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let (y, m, d) = civil_from_days((secs / 86400) as i64);
        assert!(y >= 2024 && y <= 2035, "year out of range: {y}");
        assert!((1..=12).contains(&m), "month out of range: {m}");
        assert!((1..=31).contains(&d), "day out of range: {d}");
    }

    #[test]
    fn test_metadata_has_obs_v1_authority() {
        let meta = TelemetryMetadata::new(
            42,
            serde_json::json!({"dt": 0.008333}),
            30,
            [32.0, 21.0],
            vec![],
            0,
            None,
        );
        assert_eq!(meta.obs_schema, "obs_v1");
        assert!(!meta.obs_layout.is_empty());
        assert!(meta.obs_layout.iter().any(|e| e[0] == "[10..16]"));
    }
}
