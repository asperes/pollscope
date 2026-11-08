# Spec Delta

## Purpose

Turns one device's sequence of report timestamps into interval statistics and verdicts about gaps, batching and whether there is enough data to trust the numbers.

## ADDED Requirements

### Requirement: Compute report intervals
The system SHALL compute the interval between each pair of consecutive arrivals from the same device, where an arrival is one report or one batch of reports delivered in a single read. It SHALL never compute an interval across reports from different devices.

#### Scenario: Single device
- **WHEN** a device has reports at 0 µs, 1000 µs and 2000 µs
- **THEN** its intervals are 1000 µs and 1000 µs

#### Scenario: Batched reports
- **WHEN** three reports from one device arrive in a single read at 5000 µs, after a previous arrival at 4000 µs
- **THEN** one interval of 1000 µs is computed and no 0 µs intervals are produced for that batch

#### Scenario: Two interleaved devices
- **WHEN** reports from device A and device B alternate in time
- **THEN** intervals for A are computed only between A's reports, and likewise for B

### Requirement: Exclude idle pauses
The system SHALL treat an interval as an idle pause, not a delivery gap, when it is at least 20 times the median of all the run's intervals or at least 50 ms, and SHALL exclude idle pauses from all timing statistics while reporting their count.

#### Scenario: User stops moving
- **WHEN** a 1000 Hz mouse is held still for 300 ms in the middle of a run
- **THEN** that 300 ms interval is counted as one idle pause and is not counted as a gap or used in jitter

### Requirement: Summarize interval statistics
For each analysed run, the system SHALL report the number of intervals used, the median interval, the implied report rate (1 / median), and the 50th, 95th and 99th percentiles of each interval's absolute deviation from the median. Percentiles, including the median, use the nearest-rank method.

#### Scenario: Perfectly steady stream
- **WHEN** every used interval is exactly 1000 µs
- **THEN** the median is 1000 µs, the implied rate is 1000 Hz, and every deviation percentile is 0 µs

#### Scenario: Mostly steady with outliers
- **WHEN** 980 intervals are 1000 µs and 20 intervals are 3000 µs
- **THEN** the median is 1000 µs, the 95th-percentile deviation is 0 µs and the 99th-percentile deviation is 2000 µs

### Requirement: Count delivery gaps
The system SHALL count as a gap every non-idle interval of at least 2 times the median interval, and SHALL report the gap count, the longest gap, and gaps per 10,000 intervals.

#### Scenario: Missed slot
- **WHEN** a 1000 Hz stream contains one 2100 µs interval
- **THEN** the gap count is 1 and the longest gap is 2100 µs

#### Scenario: Normal variation
- **WHEN** intervals vary between 900 µs and 1100 µs around a 1000 µs median
- **THEN** the gap count is 0

### Requirement: Report batched delivery
The system SHALL report how many reports in the run arrived batched (more than one report from the device in a single read) and what share of all reports that is.

#### Scenario: Some batching
- **WHEN** 50 of 10,000 reports arrived in multi-report reads
- **THEN** the result shows 50 batched reports (0.5%)

### Requirement: Withhold results without enough data
The system SHALL mark a result "not enough data" and SHALL NOT show the rate, deviation or gap figures when fewer than 500 non-idle intervals were collected.

#### Scenario: Too little movement
- **WHEN** a 10-second run yields only 200 non-idle intervals
- **THEN** the result is marked "not enough data" and no rate or jitter numbers are shown

### Requirement: Flag untrustworthy runs
The system SHALL mark a result as not trustworthy, and say why, when reports were dropped by capture or the run ended early.

#### Scenario: Dropped reports
- **WHEN** capture reports any dropped reports for the run
- **THEN** the result is marked not trustworthy with the reason "reports were dropped during capture"

### Requirement: Deterministic analysis
The system SHALL produce identical results when the same recorded timestamps are analysed again, independent of the machine or operating system doing the analysis.

#### Scenario: Re-analysis of an export
- **WHEN** an exported run's timestamps are analysed twice, once on Windows and once on another platform
- **THEN** both analyses produce the same figures
