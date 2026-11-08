//! Re-analysing the same recording must give identical results.

use analysis::{DeviceKey, analyze, synth};
use proptest::prelude::*;

proptest! {
    #[test]
    fn reanalysis_is_identical(
        intervals in prop::collection::vec(1u64..200_000, 0..3_000),
        dropped in 0u64..3,
    ) {
        let mut run = synth::run_from_interval_ticks(DeviceKey(1), &intervals);
        run.dropped_reports = dropped;
        let first = analyze(&run);
        let json = serde_json::to_string(&run).unwrap();
        let reloaded: analysis::RecordedRun = serde_json::from_str(&json).unwrap();
        let second = analyze(&reloaded);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn batching_never_creates_zero_intervals(
        sizes in prop::collection::vec(1usize..5, 1..500),
    ) {
        let mut run = synth::run_from_interval_ticks(DeviceKey(1), &vec![10_000; sizes.len() - 1]);
        let mut events = Vec::new();
        for (e, n) in run.events.iter().zip(&sizes) {
            for _ in 0..*n { events.push(*e); }
        }
        run.events = events;
        let intervals = analysis::arrival_intervals_ns(&run, DeviceKey(1));
        prop_assert!(intervals.iter().all(|&i| i > 0));
        prop_assert_eq!(intervals.len(), sizes.len() - 1);
    }
}
