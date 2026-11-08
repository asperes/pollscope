//! JSON and CSV export of a finished run.
//!
//! Exports contain no Windows user name, computer name or device instance
//! path. The device is identified by its product name and a hashed id.

use std::io::Write;

use analysis::{RecordedRun, TimingResult};
use serde::Serialize;

use crate::selection::Device;

pub const APP_NAME: &str = "pollscope";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
struct ExportDevice<'a> {
    name: &'a str,
    export_id: &'a str,
}

#[derive(Serialize)]
struct ExportDoc<'a> {
    app: &'static str,
    app_version: &'static str,
    scope: &'static str,
    device: ExportDevice<'a>,
    run: &'a RecordedRun,
    result: &'a TimingResult,
}

/// Writes the full run, every timestamp included, and its result as JSON.
pub fn write_json(
    w: impl Write,
    device: &Device,
    run: &RecordedRun,
    result: &TimingResult,
) -> serde_json::Result<()> {
    let doc = ExportDoc {
        app: APP_NAME,
        app_version: APP_VERSION,
        scope: crate::format::SCOPE_STATEMENT,
        device: ExportDevice {
            name: &device.name,
            export_id: &device.export_id,
        },
        run,
        result,
    };
    serde_json::to_writer_pretty(w, &doc)
}

/// Writes one CSV row per recorded report.
pub fn write_csv(w: impl Write, device: &Device, run: &RecordedRun) -> csv::Result<()> {
    let mut out = csv::Writer::from_writer(w);
    out.write_record([
        "device_export_id",
        "report",
        "batch_id",
        "qpc_ticks",
        "time_us",
        "dx",
        "dy",
        "button_flags",
    ])?;
    let first = run.events.first().map_or(0, |e| e.ticks);
    let freq = run.qpc_frequency.max(1) as f64;
    for (i, e) in run.events.iter().enumerate() {
        let t_us = (e.ticks - first) as f64 * 1e6 / freq;
        out.write_record([
            device.export_id.clone(),
            i.to_string(),
            e.batch_id.to_string(),
            e.ticks.to_string(),
            format!("{t_us:.3}"),
            e.dx.to_string(),
            e.dy.to_string(),
            e.buttons.to_string(),
        ])?;
    }
    out.flush()?;
    Ok(())
}

/// Suggested file name, without user or machine identifiers.
pub fn suggested_name(extension: &str) -> String {
    format!("{APP_NAME}-run.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use analysis::{DeviceKey, analyze, synth};

    fn fixture() -> (Device, RecordedRun, TimingResult) {
        let run = synth::jittery(DeviceKey(0x1_0045), 1000, 700, 500, 3);
        let result = analyze(&run);
        let device = Device {
            key: DeviceKey(0x1_0045),
            name: "Test Mouse".into(),
            export_id: "a1b2c3d4e5f6".into(),
        };
        (device, run, result)
    }

    #[test]
    fn json_contains_timestamps_and_figures() {
        let (device, run, result) = fixture();
        let mut buf = Vec::new();
        write_json(&mut buf, &device, &run, &result).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["device"]["export_id"], "a1b2c3d4e5f6");
        assert_eq!(v["run"]["events"].as_array().unwrap().len(), 700);
        assert_eq!(
            v["result"]["stats"]["median_ns"].as_u64(),
            result.stats.as_ref().map(|s| s.median_ns)
        );
        let back: RecordedRun = serde_json::from_value(v["run"].clone()).unwrap();
        assert_eq!(analyze(&back), result, "export re-analyses identically");
    }

    #[test]
    fn csv_has_one_row_per_report() {
        let (device, run, _) = fixture();
        let mut buf = Vec::new();
        write_csv(&mut buf, &device, &run).unwrap();
        let text = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 701);
        assert!(lines[0].starts_with("device_export_id,report,batch_id"));
        assert!(lines[1].starts_with("a1b2c3d4e5f6,0,0,"));
        assert!(lines[1].contains(",0.000,"));
    }

    #[test]
    fn no_personal_identifiers() {
        let (device, run, result) = fixture();
        let mut json = Vec::new();
        write_json(&mut json, &device, &run, &result).unwrap();
        let mut csv = Vec::new();
        write_csv(&mut csv, &device, &run).unwrap();
        for text in [
            String::from_utf8(json).unwrap(),
            String::from_utf8(csv).unwrap(),
        ] {
            let lower = text.to_lowercase();
            for needle in ["username", "computername", "\\\\?\\", "hid#", "vid_"] {
                assert!(!lower.contains(needle), "{needle} found");
            }
            for var in ["USERNAME", "COMPUTERNAME", "USER", "HOSTNAME"] {
                if let Ok(v) = std::env::var(var)
                    && v.len() >= 3
                {
                    assert!(!text.contains(&v), "{var} value leaked");
                }
            }
        }
        assert_eq!(suggested_name("json"), "pollscope-run.json");
    }
}
