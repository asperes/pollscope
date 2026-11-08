//! Golden results for synthetic fixtures. These run on macOS and Windows CI;
//! identical expectations on both platforms check the deterministic-analysis
//! requirement.

use analysis::{DeviceKey, analyze, synth};

const DEV: DeviceKey = DeviceKey(42);

#[test]
fn steady_1khz() {
    let r = analyze(&synth::steady(DEV, 1000, 10_001));
    let s = r.stats.as_ref().expect("enough data");
    assert_eq!(s.intervals, 10_000);
    assert_eq!(s.median_ns, 1_000_000);
    assert_eq!(s.rate_hz, 1000.0);
    assert_eq!(s.deviation_p99_ns, 0);
    assert_eq!(s.gaps, 0);
    assert!(r.trustworthy());
}

#[test]
fn jittery_8khz() {
    // 8 kHz = 1250 ticks at 10 MHz; jitter ±300 ticks (±30 µs).
    let r = analyze(&synth::jittery(DEV, 8000, 80_001, 300, 0xC0FFEE));
    let s = r.stats.as_ref().expect("enough data");
    assert_eq!(s.intervals, 80_000);
    // Uniform ±30 µs around 125 µs: median within a tick of 125 µs.
    assert!(s.median_ns.abs_diff(125_000) <= 100, "{}", s.median_ns);
    // Deviations are bounded by the jitter and roughly uniform.
    assert!(s.deviation_p99_ns <= 30_000 + 100);
    assert!(s.deviation_p50_ns.abs_diff(15_000) <= 1_000);
    assert_eq!(s.gaps, 0);
}

#[test]
fn idle_and_gaps_1khz() {
    let r = analyze(&synth::with_idle_and_gaps(DEV, 5_001, 3, 4));
    assert_eq!(r.idle_pauses, 3);
    let s = r.stats.as_ref().expect("enough data");
    assert_eq!(s.intervals, 4_997);
    assert_eq!(s.gaps, 4);
    assert_eq!(s.longest_gap_ns, Some(2_000_000));
}

#[test]
fn jittery_golden_values_are_platform_independent() {
    // Exact values: any platform-dependent arithmetic would change them.
    let s = analyze(&synth::jittery(DEV, 1000, 2_001, 2_000, 7))
        .stats
        .expect("enough data");
    let got = (
        s.median_ns,
        s.deviation_p50_ns,
        s.deviation_p95_ns,
        s.deviation_p99_ns,
    );
    assert_eq!(
        got, GOLDEN_JITTERY_1KHZ,
        "update GOLDEN only if the algorithm changed on purpose"
    );
}

// Recorded on macOS (aarch64); Windows CI must reproduce them exactly.
const GOLDEN_JITTERY_1KHZ: (u64, u64, u64, u64) = (998_800, 97_800, 189_100, 198_300);
