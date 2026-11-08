//! Interval statistics, gap and batch detection, and data-sufficiency verdicts.

use serde::{Deserialize, Serialize};

use crate::model::{DeviceKey, RecordedRun, RunEnd};

/// An interval at least this many times the median of all intervals is an idle pause.
pub const IDLE_FACTOR: u64 = 20;
/// An interval at least this long (50 ms) is always an idle pause.
pub const IDLE_FLOOR_NS: u64 = 50_000_000;
/// A non-idle interval at least this many times the median is a delivery gap.
pub const GAP_FACTOR: u64 = 2;
/// Fewer non-idle intervals than this means "not enough data".
pub const MIN_INTERVALS: u64 = 500;

/// Why a result should not be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustIssue {
    DroppedReports,
    EndedEarly,
}

impl TrustIssue {
    /// Plain-language reason shown next to the figures.
    pub fn reason(self) -> &'static str {
        match self {
            TrustIssue::DroppedReports => "reports were dropped during capture",
            TrustIssue::EndedEarly => "the run ended early because the mouse was disconnected",
        }
    }
}

/// Figures shown only when there was enough data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingStats {
    /// Non-idle intervals the figures are based on.
    pub intervals: u64,
    pub median_ns: u64,
    /// 1 / median, in Hz.
    pub rate_hz: f64,
    /// Nearest-rank percentiles of |interval - median|.
    pub deviation_p50_ns: u64,
    pub deviation_p95_ns: u64,
    pub deviation_p99_ns: u64,
    pub gaps: u64,
    /// Longest gap, if there was at least one.
    pub longest_gap_ns: Option<u64>,
    pub gaps_per_10k: f64,
}

/// Analysis result for one device in one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimingResult {
    pub device: DeviceKey,
    /// Reports from this device in the run.
    pub reports: u64,
    /// Arrivals: single reports or batches delivered in one read.
    pub arrivals: u64,
    /// Reports that arrived together with another report from this device.
    pub batched_reports: u64,
    /// `batched_reports / reports` (0 when there were no reports).
    pub batched_share: f64,
    /// Intervals classified as idle pauses and excluded from the figures.
    pub idle_pauses: u64,
    pub dropped_reports: u64,
    /// `None` means "not enough data".
    pub stats: Option<TimingStats>,
    /// Empty means trustworthy.
    pub trust_issues: Vec<TrustIssue>,
}

impl TimingResult {
    pub fn enough_data(&self) -> bool {
        self.stats.is_some()
    }

    pub fn trustworthy(&self) -> bool {
        self.trust_issues.is_empty()
    }
}

/// Arrival timestamps (ticks) and per-arrival report counts for one device.
fn arrivals(run: &RecordedRun, device: DeviceKey) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = Vec::new();
    let mut last_batch: Option<u64> = None;
    for e in run.events.iter().filter(|e| e.device == device) {
        if last_batch == Some(e.batch_id) {
            if let Some(last) = out.last_mut() {
                last.1 += 1;
            }
        } else {
            out.push((e.ticks, 1));
            last_batch = Some(e.batch_id);
        }
    }
    out
}

fn ticks_to_ns(ticks: u64, frequency: u64) -> u64 {
    debug_assert!(frequency > 0);
    let ns = u128::from(ticks) * 1_000_000_000 / u128::from(frequency.max(1));
    u64::try_from(ns).unwrap_or(u64::MAX)
}

/// Intervals between consecutive arrivals of one device, in nanoseconds.
///
/// Reports delivered in one read count as a single arrival, so batches never
/// produce 0 ns intervals. Reports of other devices are ignored.
pub fn arrival_intervals_ns(run: &RecordedRun, device: DeviceKey) -> Vec<u64> {
    arrivals(run, device)
        .windows(2)
        .map(|w| ticks_to_ns(w[1].0.saturating_sub(w[0].0), run.qpc_frequency))
        .collect()
}

/// Nearest-rank percentile of an ascending slice. `p` is in percent (1..=100).
fn nearest_rank(sorted: &[u64], p: u64) -> u64 {
    debug_assert!(!sorted.is_empty() && (1..=100).contains(&p));
    let n = sorted.len() as u64;
    let rank = (p * n).div_ceil(100).max(1);
    sorted[(rank - 1) as usize]
}

fn median(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    Some(nearest_rank(&sorted, 50))
}

/// Analyse the measured device of `run`.
pub fn analyze(run: &RecordedRun) -> TimingResult {
    let device = run.settings.device;
    let arrivals = arrivals(run, device);
    let reports: u64 = arrivals.iter().map(|a| a.1).sum();
    let batched_reports: u64 = arrivals.iter().filter(|a| a.1 > 1).map(|a| a.1).sum();
    let intervals = arrival_intervals_ns(run, device);

    let (used, idle_pauses) = match median(&intervals) {
        Some(all_median) => {
            let idle_limit = all_median.saturating_mul(IDLE_FACTOR).min(IDLE_FLOOR_NS);
            let used: Vec<u64> = intervals
                .iter()
                .copied()
                .filter(|&i| i < idle_limit)
                .collect();
            let idle = intervals.len() as u64 - used.len() as u64;
            (used, idle)
        }
        None => (Vec::new(), 0),
    };

    let stats = if (used.len() as u64) < MIN_INTERVALS {
        None
    } else {
        Some(summarize(&used))
    };

    let mut trust_issues = Vec::new();
    if run.dropped_reports > 0 {
        trust_issues.push(TrustIssue::DroppedReports);
    }
    if run.end != RunEnd::Completed {
        trust_issues.push(TrustIssue::EndedEarly);
    }

    TimingResult {
        device,
        reports,
        arrivals: arrivals.len() as u64,
        batched_reports,
        batched_share: if reports == 0 {
            0.0
        } else {
            batched_reports as f64 / reports as f64
        },
        idle_pauses,
        dropped_reports: run.dropped_reports,
        stats,
        trust_issues,
    }
}

fn summarize(used: &[u64]) -> TimingStats {
    let mut sorted = used.to_vec();
    sorted.sort_unstable();
    let median_ns = nearest_rank(&sorted, 50);

    let mut deviations: Vec<u64> = sorted.iter().map(|&i| i.abs_diff(median_ns)).collect();
    deviations.sort_unstable();

    let gap_limit = median_ns.saturating_mul(GAP_FACTOR);
    let gaps: Vec<u64> = sorted.iter().copied().filter(|&i| i >= gap_limit).collect();
    let n = sorted.len() as u64;

    TimingStats {
        intervals: n,
        median_ns,
        rate_hz: if median_ns == 0 {
            0.0
        } else {
            1e9 / median_ns as f64
        },
        deviation_p50_ns: nearest_rank(&deviations, 50),
        deviation_p95_ns: nearest_rank(&deviations, 95),
        deviation_p99_ns: nearest_rank(&deviations, 99),
        gaps: gaps.len() as u64,
        longest_gap_ns: gaps.last().copied(),
        gaps_per_10k: gaps.len() as f64 * 10_000.0 / n as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MouseEvent, RunSettings};

    /// 1 tick = 1 µs, so tick values in tests read directly as microseconds.
    const FREQ: u64 = 1_000_000;
    const DEV: DeviceKey = DeviceKey(1);

    fn run_from(events: Vec<MouseEvent>) -> RecordedRun {
        RecordedRun {
            qpc_frequency: FREQ,
            settings: RunSettings {
                device: DEV,
                duration_secs: 10,
            },
            events,
            dropped_reports: 0,
            end: RunEnd::Completed,
        }
    }

    fn ev(device: DeviceKey, us: u64, batch_id: u64) -> MouseEvent {
        MouseEvent {
            device,
            ticks: us,
            batch_id,
            dx: 1,
            dy: 0,
            buttons: 0,
        }
    }

    /// A run with one report per read at the given gaps (µs) after t=0.
    fn run_from_intervals_us(intervals: &[u64]) -> RecordedRun {
        let mut t = 0;
        let mut events = vec![ev(DEV, 0, 0)];
        for (i, d) in intervals.iter().enumerate() {
            t += d;
            events.push(ev(DEV, t, i as u64 + 1));
        }
        run_from(events)
    }

    #[test]
    fn single_device_intervals() {
        let run = run_from(vec![ev(DEV, 0, 0), ev(DEV, 1000, 1), ev(DEV, 2000, 2)]);
        assert_eq!(arrival_intervals_ns(&run, DEV), vec![1_000_000, 1_000_000]);
    }

    #[test]
    fn interleaved_devices_never_mix() {
        let b = DeviceKey(2);
        let run = run_from(vec![
            ev(DEV, 0, 0),
            ev(b, 300, 1),
            ev(DEV, 1000, 2),
            ev(b, 2300, 3),
            ev(DEV, 2000, 4),
        ]);
        assert_eq!(arrival_intervals_ns(&run, DEV), vec![1_000_000, 1_000_000]);
        assert_eq!(arrival_intervals_ns(&run, b), vec![2_000_000]);
    }

    #[test]
    fn batched_reports_are_one_arrival() {
        let run = run_from(vec![
            ev(DEV, 4000, 0),
            ev(DEV, 5000, 1),
            ev(DEV, 5000, 1),
            ev(DEV, 5000, 1),
        ]);
        assert_eq!(arrival_intervals_ns(&run, DEV), vec![1_000_000]);
        let r = analyze(&run);
        assert_eq!(r.reports, 4);
        assert_eq!(r.arrivals, 2);
        assert_eq!(r.batched_reports, 3);
    }

    #[test]
    fn batch_share_counts_reports() {
        // 9,950 single arrivals plus 25 batches of 2 = 50 batched of 10,000.
        let mut events = Vec::new();
        let mut t = 0;
        for batch in 0..9_975u64 {
            events.push(ev(DEV, t, batch));
            if batch < 25 {
                events.push(ev(DEV, t, batch));
            }
            t += 1000;
        }
        let r = analyze(&run_from(events));
        assert_eq!(r.reports, 10_000);
        assert_eq!(r.batched_reports, 50);
        assert!((r.batched_share - 0.005).abs() < 1e-12);
    }

    #[test]
    fn perfectly_steady_stream() {
        let r = analyze(&run_from_intervals_us(&[1000; 1000]));
        let s = r.stats.expect("enough data");
        assert_eq!(s.median_ns, 1_000_000);
        assert_eq!(s.rate_hz, 1000.0);
        assert_eq!(
            (s.deviation_p50_ns, s.deviation_p95_ns, s.deviation_p99_ns),
            (0, 0, 0)
        );
        assert_eq!(s.gaps, 0);
        assert_eq!(s.longest_gap_ns, None);
    }

    #[test]
    fn mostly_steady_with_outliers() {
        let mut iv = vec![1000; 980];
        iv.extend([3000; 20]);
        let s = analyze(&run_from_intervals_us(&iv))
            .stats
            .expect("enough data");
        assert_eq!(s.median_ns, 1_000_000);
        assert_eq!(s.deviation_p95_ns, 0);
        assert_eq!(s.deviation_p99_ns, 2_000_000);
    }

    #[test]
    fn user_stops_moving_is_idle_not_gap() {
        let mut iv = vec![1000; 600];
        iv.push(300_000);
        iv.extend([1000; 600]);
        let r = analyze(&run_from_intervals_us(&iv));
        assert_eq!(r.idle_pauses, 1);
        let s = r.stats.expect("enough data");
        assert_eq!(s.intervals, 1200);
        assert_eq!(s.gaps, 0);
        assert_eq!(s.deviation_p99_ns, 0);
    }

    #[test]
    fn idle_by_factor_below_floor() {
        // 125 µs median (8 kHz): 20x = 2.5 ms counts as idle even though < 50 ms.
        let mut iv = vec![125; 800];
        iv.push(2_500);
        iv.push(2_400);
        let r = analyze(&run_from_intervals_us(&iv));
        assert_eq!(r.idle_pauses, 1);
        assert_eq!(r.stats.expect("enough data").gaps, 1);
    }

    #[test]
    fn missed_slot_is_a_gap() {
        let mut iv = vec![1000; 700];
        iv.insert(350, 2100);
        let s = analyze(&run_from_intervals_us(&iv))
            .stats
            .expect("enough data");
        assert_eq!(s.gaps, 1);
        assert_eq!(s.longest_gap_ns, Some(2_100_000));
        assert!((s.gaps_per_10k - 10_000.0 / 701.0).abs() < 1e-9);
    }

    #[test]
    fn normal_variation_has_no_gaps() {
        let iv: Vec<u64> = (0..1000).map(|i| 900 + (i * 37) % 201).collect();
        let s = analyze(&run_from_intervals_us(&iv))
            .stats
            .expect("enough data");
        assert_eq!(s.gaps, 0);
    }

    #[test]
    fn not_enough_data() {
        let r = analyze(&run_from_intervals_us(&[1000; 200]));
        assert!(!r.enough_data());
        assert!(r.stats.is_none());
        let r = analyze(&run_from_intervals_us(&[1000; 499]));
        assert!(!r.enough_data());
        let r = analyze(&run_from_intervals_us(&[1000; 500]));
        assert!(r.enough_data());
    }

    #[test]
    fn empty_run() {
        let r = analyze(&run_from(Vec::new()));
        assert_eq!((r.reports, r.arrivals, r.idle_pauses), (0, 0, 0));
        assert!(!r.enough_data());
        assert_eq!(r.batched_share, 0.0);
    }

    #[test]
    fn dropped_reports_make_run_untrustworthy() {
        let mut run = run_from_intervals_us(&[1000; 1000]);
        run.dropped_reports = 3;
        let r = analyze(&run);
        assert!(!r.trustworthy());
        assert_eq!(r.trust_issues, vec![TrustIssue::DroppedReports]);
        assert_eq!(
            r.trust_issues[0].reason(),
            "reports were dropped during capture"
        );
    }

    #[test]
    fn early_end_makes_run_untrustworthy() {
        let mut run = run_from_intervals_us(&[1000; 1000]);
        run.end = RunEnd::DeviceRemoved;
        let r = analyze(&run);
        assert_eq!(r.trust_issues, vec![TrustIssue::EndedEarly]);
    }

    #[test]
    fn nearest_rank_definition() {
        let v = [10, 20, 30, 40];
        assert_eq!(nearest_rank(&v, 50), 20);
        assert_eq!(nearest_rank(&v, 75), 30);
        assert_eq!(nearest_rank(&v, 100), 40);
        assert_eq!(nearest_rank(&[5], 1), 5);
    }

    #[test]
    fn tick_conversion_uses_frequency() {
        // Typical 10 MHz QPC: 10,000 ticks = 1 ms.
        assert_eq!(ticks_to_ns(10_000, 10_000_000), 1_000_000);
        assert_eq!(ticks_to_ns(u64::MAX, 1), u64::MAX);
    }
}
