//! Recorded-run data model shared by capture, analysis, UI and export.

use serde::{Deserialize, Serialize};

/// Session-local identifier of one input device.
///
/// Capture assigns it; it is stable while the app runs and means nothing
/// outside that session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceKey(pub u64);

/// One mouse report as Windows delivered it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseEvent {
    /// Device that produced the report.
    pub device: DeviceKey,
    /// Host timestamp in QPC ticks, read once per read call when the read returned.
    pub ticks: u64,
    /// Identifies the read call that delivered this report. Reports with the
    /// same `batch_id` arrived together; ids increase with every read.
    pub batch_id: u64,
    /// Relative motion as reported.
    pub dx: i32,
    pub dy: i32,
    /// Raw button transition flags of the report (0 when only motion).
    pub buttons: u16,
}

/// Settings the user chose for a timed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSettings {
    /// Device being measured.
    pub device: DeviceKey,
    /// Requested run length in seconds.
    pub duration_secs: u32,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunEnd {
    /// Ran for the full requested duration.
    Completed,
    /// The measured device was disconnected before the run finished.
    DeviceRemoved,
}

/// Everything recorded during one run: the input to [`crate::analyze`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedRun {
    /// QPC ticks per second on the recording machine.
    pub qpc_frequency: u64,
    pub settings: RunSettings,
    /// Reports in arrival order. May include other devices; analysis filters.
    pub events: Vec<MouseEvent>,
    /// Reports capture had to drop because its buffer was full.
    pub dropped_reports: u64,
    pub end: RunEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_run_json_round_trip() {
        let run = RecordedRun {
            qpc_frequency: 10_000_000,
            settings: RunSettings {
                device: DeviceKey(7),
                duration_secs: 10,
            },
            events: vec![
                MouseEvent {
                    device: DeviceKey(7),
                    ticks: 123,
                    batch_id: 0,
                    dx: -3,
                    dy: 4,
                    buttons: 0,
                },
                MouseEvent {
                    device: DeviceKey(7),
                    ticks: 10_123,
                    batch_id: 1,
                    dx: 1,
                    dy: 0,
                    buttons: 0x0001,
                },
            ],
            dropped_reports: 2,
            end: RunEnd::DeviceRemoved,
        };
        let json = serde_json::to_string(&run).expect("serialize");
        assert!(json.contains("\"device_removed\""));
        let back: RecordedRun = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, run);
    }
}
