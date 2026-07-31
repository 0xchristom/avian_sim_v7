use std::path::Path;
use std::collections::VecDeque;
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub time_us: u64,
    pub frame: u32,
}

pub struct TelemetryExporter {
    buffer: VecDeque<TelemetryFrame>,
    max_frames: usize,
}

impl TelemetryExporter {
    pub fn new(max_frames: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(max_frames),
            max_frames,
        }
    }

    pub fn push(&mut self, frame: TelemetryFrame) {
        if self.buffer.len() >= self.max_frames {
            self.buffer.pop_front();
        }
        self.buffer.push_back(frame);
    }

    pub fn flush_to_parquet(&self, _path: &Path) {
        // TODO: Implementation uses arrow::array::RecordBatch and parquet::arrow::ArrowWriter
    }

    pub async fn stream_to_clickhouse(&self, _url: &str) {
        // TODO: Async insertion stub
    }
}