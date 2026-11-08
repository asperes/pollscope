//! Deterministic synthetic recordings for tests and fixtures.
//!
//! These streams aren't meant to model any real mouse. They give known shapes
//! (steady, jittery, with idle pauses and missed slots) whose expected
//! analysis results can be written down exactly.

use crate::model::{DeviceKey, MouseEvent, RecordedRun, RunEnd, RunSettings};

/// QPC frequency used by synthetic runs (10 MHz, the common Windows value).
pub const SYNTH_QPC_FREQUENCY: u64 = 10_000_000;

/// Small deterministic PRNG (SplitMix64), so fixtures never depend on a crate's RNG.
#[derive(Debug, Clone)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `-bound..=bound`.
    pub fn symmetric(&mut self, bound: u64) -> i64 {
        if bound == 0 {
            return 0;
        }
        let span = bound * 2 + 1;
        (self.next_u64() % span) as i64 - bound as i64
    }
}

/// Builds a recorded run from per-arrival intervals given in QPC ticks.
pub fn run_from_interval_ticks(device: DeviceKey, intervals: &[u64]) -> RecordedRun {
    let mut events = Vec::with_capacity(intervals.len() + 1);
    let mut t = 1_000_000;
    let push = |events: &mut Vec<MouseEvent>, t: u64, batch_id: u64| {
        events.push(MouseEvent {
            device,
            ticks: t,
            batch_id,
            dx: 1,
            dy: 0,
            buttons: 0,
        });
    };
    push(&mut events, t, 0);
    for (i, d) in intervals.iter().enumerate() {
        t += d;
        push(&mut events, t, i as u64 + 1);
    }
    RecordedRun {
        qpc_frequency: SYNTH_QPC_FREQUENCY,
        settings: RunSettings {
            device,
            duration_secs: 10,
        },
        events,
        dropped_reports: 0,
        end: RunEnd::Completed,
    }
}

/// Perfectly steady stream at `rate_hz` with `arrivals` arrivals.
pub fn steady(device: DeviceKey, rate_hz: u64, arrivals: usize) -> RecordedRun {
    let period = SYNTH_QPC_FREQUENCY / rate_hz;
    run_from_interval_ticks(device, &vec![period; arrivals.saturating_sub(1)])
}

/// Stream at `rate_hz` whose intervals vary uniformly by ±`jitter_ticks`.
pub fn jittery(
    device: DeviceKey,
    rate_hz: u64,
    arrivals: usize,
    jitter_ticks: u64,
    seed: u64,
) -> RecordedRun {
    let period = (SYNTH_QPC_FREQUENCY / rate_hz) as i64;
    let mut rng = SplitMix64::new(seed);
    let intervals: Vec<u64> = (1..arrivals)
        .map(|_| (period + rng.symmetric(jitter_ticks)).max(1) as u64)
        .collect();
    run_from_interval_ticks(device, &intervals)
}

/// 1 kHz stream with `pauses` idle pauses of 300 ms and `missed` single missed slots.
pub fn with_idle_and_gaps(
    device: DeviceKey,
    arrivals: usize,
    pauses: usize,
    missed: usize,
) -> RecordedRun {
    let period = SYNTH_QPC_FREQUENCY / 1000;
    let mut intervals = vec![period; arrivals.saturating_sub(1)];
    let n = intervals.len();
    for k in 0..pauses {
        intervals[(k + 1) * n / (pauses + 1)] = SYNTH_QPC_FREQUENCY * 3 / 10;
    }
    for k in 0..missed {
        let i = ((k + 1) * n / (missed + 1) + 7).min(n - 1);
        intervals[i] = period * 2;
    }
    run_from_interval_ticks(device, &intervals)
}
