//! Plain-language text for results and the measurement scope.

use analysis::TimingResult;

/// Shown on the result view and in About.
pub const SCOPE_STATEMENT: &str = "These figures show when Windows delivered this mouse's reports \
to this app on this PC. They are not the mouse's internal polling or scan rate, and not \
click-to-screen latency.";

/// One labelled row of the result table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
}

fn us(ns: u64) -> String {
    format!("{:.1} µs", ns as f64 / 1000.0)
}

fn pct(share: f64) -> String {
    format!("{:.2}%", share * 100.0)
}

/// Headline verdict: `None` when the figures can be read as they are.
pub fn verdict(result: &TimingResult) -> Option<String> {
    if !result.enough_data() {
        return Some(
            "Not enough data: keep moving the mouse for the whole run (at least 500 \
             intervals are needed)."
                .to_owned(),
        );
    }
    if !result.trustworthy() {
        let reasons: Vec<&str> = result.trust_issues.iter().map(|i| i.reason()).collect();
        return Some(format!("Not trustworthy: {}.", reasons.join("; ")));
    }
    None
}

/// Rows of figures. Timing rows appear only when there is enough data.
pub fn rows(result: &TimingResult) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut push = |label, value: String| rows.push(Row { label, value });
    if let Some(s) = &result.stats {
        push("Report rate (from median)", format!("{:.0} Hz", s.rate_hz));
        push("Median interval", us(s.median_ns));
        push("Deviation p50", us(s.deviation_p50_ns));
        push("Deviation p95", us(s.deviation_p95_ns));
        push("Deviation p99", us(s.deviation_p99_ns));
        push(
            "Gaps (≥ 2× median)",
            format!("{} ({:.1} per 10,000)", s.gaps, s.gaps_per_10k),
        );
        push(
            "Longest gap",
            s.longest_gap_ns.map_or_else(|| "—".to_owned(), us),
        );
        push("Intervals used", s.intervals.to_string());
    }
    push(
        "Batched reports",
        format!("{} ({})", result.batched_reports, pct(result.batched_share)),
    );
    push("Idle pauses excluded", result.idle_pauses.to_string());
    push("Reports recorded", result.reports.to_string());
    push("Reports dropped", result.dropped_reports.to_string());
    rows
}

/// Rate text for the live view: "—" at rest.
pub fn live_rate(rate_hz: Option<f64>) -> String {
    rate_hz.map_or_else(|| "—".to_owned(), |r| format!("{r:.0} Hz"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use analysis::{DeviceKey, RunEnd, analyze, synth};

    fn render(result: &TimingResult) -> String {
        let mut out = String::new();
        if let Some(v) = verdict(result) {
            out.push_str(&v);
            out.push('\n');
        }
        for r in rows(result) {
            out.push_str(&format!("{}: {}\n", r.label, r.value));
        }
        out
    }

    #[test]
    fn snapshot_idle_and_gaps_fixture() {
        let r = analyze(&synth::with_idle_and_gaps(DeviceKey(1), 5_001, 3, 4));
        let expected = "\
Report rate (from median): 1000 Hz
Median interval: 1000.0 µs
Deviation p50: 0.0 µs
Deviation p95: 0.0 µs
Deviation p99: 0.0 µs
Gaps (≥ 2× median): 4 (8.0 per 10,000)
Longest gap: 2000.0 µs
Intervals used: 4997
Batched reports: 0 (0.00%)
Idle pauses excluded: 3
Reports recorded: 5001
Reports dropped: 0
";
        assert_eq!(render(&r), expected);
    }

    #[test]
    fn not_enough_data_hides_timing_rows() {
        let r = analyze(&synth::steady(DeviceKey(1), 1000, 100));
        let text = render(&r);
        assert!(text.starts_with("Not enough data"));
        assert!(!text.contains("Report rate"));
        assert!(!text.contains("Deviation"));
    }

    #[test]
    fn untrustworthy_reason_is_inline() {
        let mut run = synth::steady(DeviceKey(1), 1000, 2_000);
        run.dropped_reports = 1;
        run.end = RunEnd::DeviceRemoved;
        let text = render(&analyze(&run));
        assert!(text.starts_with(
            "Not trustworthy: reports were dropped during capture; the run ended early"
        ));
    }

    #[test]
    fn live_rate_text() {
        assert_eq!(live_rate(None), "—");
        assert_eq!(live_rate(Some(999.6)), "1000 Hz");
    }

    #[test]
    fn scope_statement_names_what_is_not_measured() {
        assert!(SCOPE_STATEMENT.contains("not the mouse's internal polling"));
        assert!(SCOPE_STATEMENT.contains("not click-to-screen latency"));
    }
}
