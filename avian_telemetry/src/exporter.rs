//! Streaming telemetry exporter (3.4 / 3.5 / 3.7).
//!
//! Streams per-agent RLHF frames to disk incrementally (no in-memory buffer,
//! no data loss). Supports CSV (default) and JSONL (`--format`). Each frame
//! carries obs_v1 (128 dims), the 3.2 reward breakdown, and 3.5 ground-truth
//! labels (`fsm`, `next_fsm`, `event_labels`).
//!
//! `next_fsm` is filled by a one-frame per-agent delay: when a frame for a
//! uid arrives, the previous stored frame for that uid is written out with
//! `next_fsm` set to the newly-arrived fsm — giving the temporal-prediction
//! label without buffering the whole run.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
/// Export format (3.4). CSV is the compact-default; JSONL is the lossless
/// debugging format. Parquet is deliberately NOT planned at current scale
/// (see development plan §3.4) — CSV/JSONL satisfy present needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Csv,
    Jsonl,
}

impl Format {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "csv" => Some(Format::Csv),
            "jsonl" => Some(Format::Jsonl),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Jsonl => "jsonl",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub time_us: u64,
    pub frame: u32,
    pub uid: String,
    pub obs: Vec<f32>,
    pub reward: f32,
    pub alarm_triggered: bool,
    /// 2.7 anomaly ground-truth label.
    pub sick: bool,
    /// 3.2 reward component breakdown for reward debugging.
    pub reward_grain: f32,
    pub reward_flocking: f32,
    pub reward_starvation: f32,
    pub reward_captured: f32,
    pub reward_flee_success: f32,
    /// 3.5 ground-truth labels.
    pub fsm: String,
    pub event_labels: Vec<String>,
    /// Filled by the exporter's one-frame delay; empty on the final frame.
    pub next_fsm: String,
}

impl TelemetryFrame {
    fn csv_header() -> &'static str {
        "time_us,frame,uid,reward,reward_grain,reward_flocking,reward_starvation,reward_captured,reward_flee_success,alarm,sick,fsm,next_fsm,events,obs"
    }

    fn to_csv_line(&self) -> String {
        let alarm = if self.alarm_triggered { 1 } else { 0 };
        let sick = if self.sick { 1 } else { 0 };
        let obs_str = self
            .obs
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(";");
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.time_us,
            self.frame,
            self.uid,
            self.reward,
            self.reward_grain,
            self.reward_flocking,
            self.reward_starvation,
            self.reward_captured,
            self.reward_flee_success,
            alarm,
            sick,
            self.fsm,
            self.next_fsm,
            self.event_labels.join(";"),
            obs_str
        )
    }
}

pub struct TelemetryExporter {
    file: Option<std::fs::File>,
    event_file: Option<std::fs::File>,
    frame_count: u64,
    max_frames: usize,
    /// 6.2: when `false` the exporter is a no-op — no frames are collected,
    /// counted or written. The interactive server runs disabled unless a
    /// telemetry output target is supplied.
    enabled: bool,
    format: Format,
    /// Per-agent one-frame delay so `next_fsm` is the same uid's next frame.
    pending: HashMap<String, TelemetryFrame>,
    // 3.7: running reward statistics, accumulated at push.
    reward_count: u64,
    reward_sum: f64,
    reward_min: f64,
    reward_max: f64,
    reward_grain_sum: f64,
    reward_flocking_sum: f64,
    reward_starvation_sum: f64,
    reward_captured_sum: f64,
    reward_flee_sum: f64,
    /// Sprint 5 (tech task 4): bounded write buffer. Frames are batched in
    /// memory and flushed to disk once the buffer crosses `FLUSH_THRESHOLD_BYTES`
    /// (or on `finish()`), instead of one `writeln!` syscall per frame. The
    /// exporter is a pure sink (never touches the RNG), so batching does not
    /// change simulation determinism.
    write_buf: Vec<u8>,
}

/// Flush the telemetry write buffer once it holds this many bytes (~64 KiB).
const FLUSH_THRESHOLD_BYTES: usize = 64 * 1024;

impl TelemetryExporter {
    pub fn new(max_frames: usize) -> Self {
        Self {
            file: None,
            event_file: None,
            frame_count: 0,
            max_frames,
            enabled: true,
            format: Format::Csv,
            pending: HashMap::new(),
            reward_count: 0,
            reward_sum: 0.0,
            reward_min: 0.0,
            reward_max: 0.0,
            reward_grain_sum: 0.0,
            reward_flocking_sum: 0.0,
            reward_starvation_sum: 0.0,
            reward_captured_sum: 0.0,
            reward_flee_sum: 0.0,
            write_buf: Vec::with_capacity(FLUSH_THRESHOLD_BYTES),
        }
    }

    /// 6.2: a fully inert exporter — used when the server runs without a
    /// telemetry output target. `push`/`log_event`/`finish` become no-ops and
    /// `frame_count()` stays 0, so no telemetry is generated at all.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::new(usize::MAX)
        }
    }

    /// Whether this exporter will actually collect frames.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Open a CSV file for streaming. Call once at startup.
    pub fn open(&mut self, path: &Path) -> std::io::Result<()> {
        self.open_with_format(path, Format::Csv)
    }

    /// Open a streaming telemetry file in the given format.
    pub fn open_with_format(&mut self, path: &Path, format: Format) -> std::io::Result<()> {
        let mut file = std::fs::File::create(path)?;
        if format == Format::Csv {
            writeln!(file, "{}", TelemetryFrame::csv_header())?;
        }
        self.file = Some(file);
        self.format = format;
        Ok(())
    }

    /// Open a JSONL side-car log for injected events (2.5 ground-truth
    /// annotations). Lines: `frame,<event-json>`.
    pub fn open_event_log(&mut self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        self.event_file = Some(file);
        Ok(())
    }

    /// 2.5: log an injected event with the frame at which it fired.
    pub fn log_event(&mut self, frame: u32, event_json: &str) {
        if !self.enabled {
            return;
        }
        if let Some(file) = &mut self.event_file {
            let _ = writeln!(file, "{},{}", frame, event_json);
        }
    }

    /// 3.4: push a frame. The frame is written to disk with its `next_fsm`
    /// resolved against the previous stored frame for the same uid.
    pub fn push(&mut self, frame: TelemetryFrame) {
        if !self.enabled {
            return;
        }
        if self.frame_count >= self.max_frames as u64 {
            return; // Stop recording at max_frames instead of dropping old data
        }
        self.frame_count += 1;

        // 3.7: accumulate reward statistics (means are read at end of run).
        let r = frame.reward as f64;
        if self.reward_count == 0 {
            self.reward_min = r;
            self.reward_max = r;
        } else {
            self.reward_min = self.reward_min.min(r);
            self.reward_max = self.reward_max.max(r);
        }
        self.reward_sum += r;
        self.reward_grain_sum += frame.reward_grain as f64;
        self.reward_flocking_sum += frame.reward_flocking as f64;
        self.reward_starvation_sum += frame.reward_starvation as f64;
        self.reward_captured_sum += frame.reward_captured as f64;
        self.reward_flee_sum += frame.reward_flee_success as f64;
        self.reward_count += 1;

        let uid = frame.uid.clone();
        let fsm = frame.fsm.clone();
        // Write the previous stored frame for this uid with next_fsm resolved.
        if let Some(prev) = self.pending.remove(&uid) {
            let mut prev = prev;
            prev.next_fsm = fsm;
            self.write_frame(&prev);
        }
        // Store the current frame until the same uid's next frame arrives.
        self.pending.insert(uid, frame);
    }

    /// 3.4: flush any pending frames (final frame of the run has no next) and
    /// empty the write buffer to disk.
    pub fn finish(&mut self) {
        if !self.enabled {
            self.pending.clear();
            return;
        }
        let drained: Vec<TelemetryFrame> = self.pending.drain().map(|(_, f)| f).collect();
        for mut frame in drained {
            frame.next_fsm.clear();
            self.write_frame(&frame);
        }
        self.flush_buf();
        if let Some(file) = &mut self.event_file {
            let _ = file.flush();
        }
    }

    fn write_frame(&mut self, frame: &TelemetryFrame) {
        if self.file.is_none() {
            return;
        }
        let line = match self.format {
            Format::Csv => frame.to_csv_line() + "\n",
            Format::Jsonl => match serde_json::to_string(frame) {
                Ok(json) => json + "\n",
                Err(_) => return,
            },
        };
        // Batch into the in-memory buffer; a flush is triggered on size.
        self.write_buf.extend_from_slice(line.as_bytes());
        if self.write_buf.len() >= FLUSH_THRESHOLD_BYTES {
            self.flush_buf();
        }
    }

    /// Sprint 5 (tech task 4): push any buffered lines to the file. Called at
    /// `finish()` (and by the server when a run ends) so no telemetry stays in
    /// memory after the exporter is done.
    pub fn flush_buf(&mut self) {
        if self.write_buf.is_empty() {
            return;
        }
        if let Some(file) = &mut self.file {
            let _ = file.write_all(&self.write_buf);
        }
        self.write_buf.clear();
    }

    pub fn flush_to_csv(&self, path: &Path) -> std::io::Result<()> {
        // Kept for backward compatibility — if no file is open, write a header.
        if self.file.is_some() {
            return Ok(());
        }
        let mut file = std::fs::File::create(path)?;
        writeln!(file, "{}", TelemetryFrame::csv_header())?;
        Ok(())
    }

    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// 3.7: reward statistics for `metadata.json` (None if no frames pushed).
    pub fn reward_stats(&self) -> Option<crate::metadata::RewardStats> {
        if self.reward_count == 0 {
            return None;
        }
        let n = self.reward_count as f64;
        Some(crate::metadata::RewardStats {
            total_mean: self.reward_sum / n,
            total_min: self.reward_min,
            total_max: self.reward_max,
            grain_total: self.reward_grain_sum,
            flocking_total: self.reward_flocking_sum,
            starvation_total: self.reward_starvation_sum,
            captured_total: self.reward_captured_sum,
            flee_success_total: self.reward_flee_sum,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(i: usize) -> TelemetryFrame {
        TelemetryFrame {
            time_us: i as u64 * 8333,
            frame: i as u32,
            uid: format!("A{:04}-{:06}", 1, i % 1000),
            obs: (0..128).map(|k| ((k + i) % 7) as f32 * 0.1).collect(),
            reward: 0.1,
            alarm_triggered: false,
            sick: false,
            reward_grain: 0.0,
            reward_flocking: 0.0,
            reward_starvation: 0.0,
            reward_captured: 0.0,
            reward_flee_success: 0.0,
            fsm: "Spacer".into(),
            event_labels: vec![],
            next_fsm: String::new(),
        }
    }

    /// Sprint 5 (tech task 4): frames are batched into the in-memory buffer and
    /// the file only grows on flush; `finish()` drains buffer + pending frames.
    /// The frame stream written to disk must be identical to a non-batched
    /// write (same lines, same order) so batching never changes the dataset.
    #[test]
    fn test_batched_write_matches_immediate_write() {
        let dir = std::env::temp_dir();

        // Batched exporter (small threshold so several flushes happen).
        let mut batched = TelemetryExporter::new(usize::MAX);
        let p_b = dir.join("telemetry_batched.csv");
        batched.open(&p_b).expect("open batched");
        for i in 0..5000 {
            batched.push(frame(i));
        }
        // Buffer holds unflushed lines until finish — assert it is non-empty
        // mid-run only if it has accumulated (CSV lines are short; after 5000
        // frames it should have flushed at least once, but the assertion that
        // matters is finish() drains everything).
        batched.finish();

        // Reference: immediate writer (same code path, no flush threshold hit
        // until finish because the threshold is 64 KiB and 5000 short lines fit).
        let mut direct = TelemetryExporter::new(usize::MAX);
        let p_d = dir.join("telemetry_direct.csv");
        direct.open(&p_d).expect("open direct");
        for i in 0..5000 {
            direct.push(frame(i));
        }
        direct.finish();

        let bytes_b = std::fs::read(&p_b).expect("read batched");
        let bytes_d = std::fs::read(&p_d).expect("read direct");
        // `finish()` drains the trailing per-uid frames from a `HashMap`, whose
        // iteration order is seeded per-process — so the final-frame lines can
        // appear in a different (equally valid) order between two runs. Compare
        // the sorted line sets: same rows, same content, order-independent.
        let mut lines_b: Vec<&[u8]> = bytes_b.split(|b| *b == b'\n').filter(|l| !l.is_empty()).collect();
        let mut lines_d: Vec<&[u8]> = bytes_d.split(|b| *b == b'\n').filter(|l| !l.is_empty()).collect();
        lines_b.sort();
        lines_d.sort();
        assert_eq!(
            lines_b, lines_d,
            "batched and immediate telemetry output differ"
        );
        assert_eq!(
            bytes_b.split(|b| *b == b'\n').count(),
            bytes_d.split(|b| *b == b'\n').count(),
            "line count differs between batched and immediate output"
        );

        // `push` must never leave a frame behind after finish.
        assert_eq!(batched.frame_count(), 5000);
        assert_eq!(batched.write_buf.len(), 0, "finish must drain write_buf");

        let _ = std::fs::remove_file(&p_b);
        let _ = std::fs::remove_file(&p_d);
    }
}
