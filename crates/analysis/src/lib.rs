//! Delivery-timing analysis of recorded mouse reports.
//!
//! Everything here is pure and deterministic. There are no OS APIs, timestamps
//! are integer QPC ticks, and every comparison and percentile uses integers.
//! Floating point appears only in derived display values such as the rate in
//! Hz. That keeps results bit-identical across machines, so the same recording
//! always yields the same figures.
#![forbid(unsafe_code)]

mod model;
mod stats;
pub mod synth;

pub use model::{DeviceKey, MouseEvent, RecordedRun, RunEnd, RunSettings};
pub use stats::{
    GAP_FACTOR, IDLE_FACTOR, IDLE_FLOOR_NS, MIN_INTERVALS, TimingResult, TimingStats, TrustIssue,
    analyze, arrival_intervals_ns,
};
