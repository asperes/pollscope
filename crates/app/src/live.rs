//! Rolling live view of the selected device: recent intervals, current rate,
//! histogram, and min/max-per-column downsampling for the plot.

use std::collections::VecDeque;

use analysis::{DeviceKey, MouseEvent};

/// How much history the live view keeps.
pub const WINDOW_SECS: u64 = 2;
/// The rate shows "—" when the newest arrival is older than this (ms).
pub const REST_AFTER_MS: u64 = 250;
const MAX_ARRIVALS: usize = 32_768;

/// One plotted point: time since the window's left edge and the interval
/// that ended at it, both in microseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub t_us: f64,
    pub interval_us: f64,
}

#[derive(Debug)]
pub struct LiveWindow {
    device: Option<DeviceKey>,
    qpc_frequency: u64,
    /// Arrival ticks (one per read batch).
    arrivals: VecDeque<u64>,
    last_batch: Option<u64>,
}

impl LiveWindow {
    pub fn new(qpc_frequency: u64) -> Self {
        Self {
            device: None,
            qpc_frequency: qpc_frequency.max(1),
            arrivals: VecDeque::new(),
            last_batch: None,
        }
    }

    /// Switches the watched device and clears history.
    pub fn watch(&mut self, device: Option<DeviceKey>) {
        if self.device != device {
            self.device = device;
            self.arrivals.clear();
            self.last_batch = None;
        }
    }

    pub fn push(&mut self, events: &[MouseEvent]) {
        let Some(device) = self.device else { return };
        for e in events.iter().filter(|e| e.device == device) {
            if self.last_batch == Some(e.batch_id) {
                continue;
            }
            self.last_batch = Some(e.batch_id);
            self.arrivals.push_back(e.ticks);
        }
        while self.arrivals.len() > MAX_ARRIVALS {
            self.arrivals.pop_front();
        }
    }

    /// Drops arrivals older than the window.
    pub fn trim(&mut self, now_ticks: u64) {
        let keep_from = now_ticks.saturating_sub(WINDOW_SECS * self.qpc_frequency);
        while self.arrivals.front().is_some_and(|&t| t < keep_from) {
            self.arrivals.pop_front();
        }
    }

    fn to_us(&self, ticks: u64) -> f64 {
        ticks as f64 * 1e6 / self.qpc_frequency as f64
    }

    /// Points for the interval-over-time plot, with t relative to `now - WINDOW`.
    pub fn points(&self, now_ticks: u64) -> Vec<Point> {
        let left = now_ticks.saturating_sub(WINDOW_SECS * self.qpc_frequency);
        self.arrivals
            .iter()
            .zip(self.arrivals.iter().skip(1))
            .map(|(&a, &b)| Point {
                t_us: self.to_us(b.saturating_sub(left)),
                interval_us: self.to_us(b - a),
            })
            .collect()
    }

    /// Current rate from the median interval of the last [`REST_AFTER_MS`],
    /// or `None` when the mouse is at rest.
    pub fn current_rate_hz(&self, now_ticks: u64) -> Option<f64> {
        let rest = REST_AFTER_MS * self.qpc_frequency / 1000;
        let newest = *self.arrivals.back()?;
        if now_ticks.saturating_sub(newest) > rest {
            return None;
        }
        let from = now_ticks.saturating_sub(rest);
        let recent: Vec<u64> = self
            .arrivals
            .iter()
            .copied()
            .filter(|&t| t >= from)
            .collect();
        let mut intervals: Vec<u64> = recent.windows(2).map(|w| w[1] - w[0]).collect();
        if intervals.len() < 2 {
            return None;
        }
        intervals.sort_unstable();
        let median = intervals[(intervals.len() - 1) / 2];
        (median > 0).then(|| self.qpc_frequency as f64 / median as f64)
    }
}

/// Histogram of intervals over `[0, max_us)` with an overflow count.
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    pub max_us: f64,
    pub bins: Vec<u32>,
    pub overflow: u32,
}

/// Bins intervals into `bins` equal buckets up to 3x their median.
pub fn histogram(points: &[Point], bins: usize) -> Option<Histogram> {
    if points.is_empty() || bins == 0 {
        return None;
    }
    let mut v: Vec<f64> = points.iter().map(|p| p.interval_us).collect();
    v.sort_by(f64::total_cmp);
    let median = v[(v.len() - 1) / 2];
    let max_us = (median * 3.0).max(1.0);
    let mut counts = vec![0u32; bins];
    let mut overflow = 0;
    for x in v {
        let i = (x / max_us * bins as f64) as usize;
        match counts.get_mut(i) {
            Some(c) => *c += 1,
            None => overflow += 1,
        }
    }
    Some(Histogram {
        max_us,
        bins: counts,
        overflow,
    })
}

/// Reduces points to at most one (min, max) interval pair per pixel column
/// across `[0, span_us)`, so drawing cost depends on width, not on rate.
pub fn min_max_columns(points: &[Point], span_us: f64, columns: usize) -> Vec<Option<(f64, f64)>> {
    let mut out = vec![None; columns];
    if columns == 0 || span_us <= 0.0 {
        return out;
    }
    for p in points {
        if !(0.0..span_us).contains(&p.t_us) {
            continue;
        }
        let c = (p.t_us / span_us * columns as f64) as usize;
        let Some(slot) = out.get_mut(c) else {
            continue;
        };
        *slot = Some(match *slot {
            None => (p.interval_us, p.interval_us),
            Some((lo, hi)) => (lo.min(p.interval_us), hi.max(p.interval_us)),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: u64 = 1_000_000; // 1 tick = 1 µs
    const D: DeviceKey = DeviceKey(1);

    fn ev(device: DeviceKey, ticks: u64, batch: u64) -> MouseEvent {
        MouseEvent {
            device,
            ticks,
            batch_id: batch,
            dx: 1,
            dy: 0,
            buttons: 0,
        }
    }

    fn steady(from: u64, to: u64, step: u64) -> Vec<MouseEvent> {
        (from..to)
            .step_by(step as usize)
            .map(|t| ev(D, t, t))
            .collect()
    }

    #[test]
    fn rate_while_moving_and_rest() {
        let mut w = LiveWindow::new(F);
        w.watch(Some(D));
        w.push(&steady(0, 1_000_000, 1000));
        let now = 999_500;
        let rate = w.current_rate_hz(now).expect("moving");
        assert!((rate - 1000.0).abs() < 1e-9);
        assert_eq!(w.current_rate_hz(999_000 + 250_001), None, "at rest");
    }

    #[test]
    fn ignores_other_devices_and_batch_duplicates() {
        let mut w = LiveWindow::new(F);
        w.watch(Some(D));
        w.push(&[
            ev(D, 0, 0),
            ev(D, 0, 0),
            ev(DeviceKey(2), 500, 1),
            ev(D, 1000, 2),
        ]);
        let p = w.points(1000);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].interval_us, 1000.0);
    }

    #[test]
    fn switching_device_clears_history() {
        let mut w = LiveWindow::new(F);
        w.watch(Some(D));
        w.push(&steady(0, 10_000, 1000));
        w.watch(Some(DeviceKey(2)));
        assert!(w.points(10_000).is_empty());
    }

    #[test]
    fn trim_keeps_only_window() {
        let mut w = LiveWindow::new(F);
        w.watch(Some(D));
        w.push(&steady(0, 5_000_000, 1000));
        w.trim(5_000_000);
        let p = w.points(5_000_000);
        assert!(p.iter().all(|p| p.t_us >= 0.0 && p.t_us <= 2_000_000.0));
        assert!(p.len() >= 1_998 && p.len() <= 2_000, "{}", p.len());
    }

    #[test]
    fn histogram_bins_around_median() {
        let pts: Vec<Point> = [1000.0, 1000.0, 1000.0, 2000.0, 9000.0]
            .iter()
            .map(|&i| Point {
                t_us: 0.0,
                interval_us: i,
            })
            .collect();
        let h = histogram(&pts, 3).unwrap();
        assert_eq!(h.max_us, 3000.0);
        assert_eq!(h.bins, vec![0, 3, 1]);
        assert_eq!(h.overflow, 1);
        assert!(histogram(&[], 3).is_none());
    }

    #[test]
    fn downsampler_keeps_extremes_per_column() {
        let pts: Vec<Point> = (0..1000)
            .map(|i| Point {
                t_us: i as f64,
                interval_us: if i == 501 {
                    5000.0
                } else {
                    1000.0 + (i % 3) as f64
                },
            })
            .collect();
        let cols = min_max_columns(&pts, 1000.0, 10);
        assert_eq!(cols.len(), 10);
        assert_eq!(cols[5], Some((1000.0, 5000.0)));
        assert_eq!(cols[0], Some((1000.0, 1002.0)));
        assert!(min_max_columns(&pts, 1000.0, 0).is_empty());
    }

    #[test]
    fn downsampler_skips_out_of_range() {
        let pts = [
            Point {
                t_us: -1.0,
                interval_us: 1.0,
            },
            Point {
                t_us: 2000.0,
                interval_us: 1.0,
            },
        ];
        assert!(min_max_columns(&pts, 1000.0, 4).iter().all(Option::is_none));
    }
}
