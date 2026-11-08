//! Timed measurement run state machine.

use analysis::{DeviceKey, MouseEvent, RecordedRun, RunEnd, RunSettings, TimingResult};

/// Run lengths offered in the UI, in seconds.
pub const DURATIONS: [u32; 3] = [5, 10, 30];

#[derive(Debug)]
pub enum RunState {
    Idle,
    Running(ActiveRun),
    Finished(Box<FinishedRun>),
}

#[derive(Debug)]
pub struct ActiveRun {
    pub settings: RunSettings,
    pub start_ticks: u64,
    pub end_ticks: u64,
    qpc_frequency: u64,
    dropped_at_start: u64,
    events: Vec<MouseEvent>,
}

#[derive(Debug)]
pub struct FinishedRun {
    pub run: RecordedRun,
    pub result: TimingResult,
}

impl ActiveRun {
    /// Whole seconds left, rounded up (for the countdown).
    pub fn seconds_left(&self, now_ticks: u64) -> u64 {
        let left = self.end_ticks.saturating_sub(now_ticks);
        left.div_ceil(self.qpc_frequency.max(1))
    }
}

impl RunState {
    pub fn is_running(&self) -> bool {
        matches!(self, RunState::Running(_))
    }

    /// Starts a run. `dropped_now` is capture's cumulative drop counter.
    pub fn start(
        device: DeviceKey,
        duration_secs: u32,
        now_ticks: u64,
        qpc_frequency: u64,
        dropped_now: u64,
    ) -> Self {
        let expected = (duration_secs as usize).saturating_mul(8_000);
        RunState::Running(ActiveRun {
            settings: RunSettings {
                device,
                duration_secs,
            },
            start_ticks: now_ticks,
            end_ticks: now_ticks + u64::from(duration_secs) * qpc_frequency,
            qpc_frequency,
            dropped_at_start: dropped_now,
            events: Vec::with_capacity(expected),
        })
    }

    /// Records reports of the measured device that fall inside the run window.
    pub fn feed(&mut self, events: &[MouseEvent]) {
        if let RunState::Running(r) = self {
            let device = r.settings.device;
            let (start, end) = (r.start_ticks, r.end_ticks);
            r.events.extend(
                events
                    .iter()
                    .filter(|e| e.device == device && e.ticks >= start && e.ticks <= end),
            );
        }
    }

    /// Finishes the run once the clock passes its end. Feed all events up to
    /// `now_ticks` first. Returns true when the run finished on this call.
    pub fn tick(&mut self, now_ticks: u64, dropped_now: u64) -> bool {
        match self {
            RunState::Running(r) if now_ticks >= r.end_ticks => {
                self.finish(RunEnd::Completed, dropped_now);
                true
            }
            _ => false,
        }
    }

    /// Ends the run early if the measured device went away.
    pub fn device_removed(&mut self, key: DeviceKey, dropped_now: u64) -> bool {
        match self {
            RunState::Running(r) if r.settings.device == key => {
                self.finish(RunEnd::DeviceRemoved, dropped_now);
                true
            }
            _ => false,
        }
    }

    /// Cancels a running run. Nothing is kept.
    pub fn cancel(&mut self) {
        if self.is_running() {
            *self = RunState::Idle;
        }
    }

    /// Clears a finished result.
    pub fn reset(&mut self) {
        if !self.is_running() {
            *self = RunState::Idle;
        }
    }

    fn finish(&mut self, end: RunEnd, dropped_now: u64) {
        let RunState::Running(r) = std::mem::replace(self, RunState::Idle) else {
            return;
        };
        let run = RecordedRun {
            qpc_frequency: r.qpc_frequency,
            settings: r.settings,
            events: r.events,
            dropped_reports: dropped_now.saturating_sub(r.dropped_at_start),
            end,
        };
        let result = analysis::analyze(&run);
        *self = RunState::Finished(Box::new(FinishedRun { run, result }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: u64 = 1_000_000;
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

    fn feed_steady(state: &mut RunState, from: u64, to: u64) {
        let events: Vec<_> = (from..to).step_by(1000).map(|t| ev(D, t, t)).collect();
        state.feed(&events);
    }

    #[test]
    fn complete_run_produces_result() {
        let mut s = RunState::start(D, 5, 100, F, 0);
        feed_steady(&mut s, 100, 5_000_100);
        assert!(!s.tick(4_999_999, 0));
        assert!(s.tick(5_000_100, 0));
        let RunState::Finished(f) = &s else {
            panic!("not finished")
        };
        assert_eq!(f.run.end, RunEnd::Completed);
        assert!(f.result.enough_data());
        assert!(f.result.trustworthy());
        assert_eq!(f.result.stats.as_ref().unwrap().median_ns, 1_000_000);
    }

    #[test]
    fn only_measured_device_inside_window_is_recorded() {
        let mut s = RunState::start(D, 5, 1_000, F, 0);
        s.feed(&[
            ev(D, 999, 0),
            ev(D, 1_000, 1),
            ev(DeviceKey(2), 2_000, 2),
            ev(D, 5_001_000, 3),
            ev(D, 5_001_001, 4),
        ]);
        s.tick(6_000_000, 0);
        let RunState::Finished(f) = &s else { panic!() };
        let ticks: Vec<u64> = f.run.events.iter().map(|e| e.ticks).collect();
        assert_eq!(ticks, vec![1_000, 5_001_000]);
    }

    #[test]
    fn cancel_discards_everything() {
        let mut s = RunState::start(D, 10, 0, F, 0);
        feed_steady(&mut s, 0, 1_000_000);
        s.cancel();
        assert!(matches!(s, RunState::Idle));
        assert!(!s.tick(20_000_000, 0));
    }

    #[test]
    fn device_removal_ends_run_early_and_flags_it() {
        let mut s = RunState::start(D, 30, 0, F, 0);
        feed_steady(&mut s, 0, 2_000_000);
        assert!(!s.device_removed(DeviceKey(9), 0), "other device ignored");
        assert!(s.device_removed(D, 0));
        let RunState::Finished(f) = &s else { panic!() };
        assert_eq!(f.run.end, RunEnd::DeviceRemoved);
        assert!(!f.result.trustworthy());
    }

    #[test]
    fn drops_during_run_are_counted_relative_to_start() {
        let mut s = RunState::start(D, 5, 0, F, 40);
        feed_steady(&mut s, 0, 5_000_000);
        s.tick(5_000_000, 43);
        let RunState::Finished(f) = &s else { panic!() };
        assert_eq!(f.run.dropped_reports, 3);
        assert!(!f.result.trustworthy());
    }

    #[test]
    fn countdown_rounds_up() {
        let RunState::Running(r) = RunState::start(D, 10, 0, F, 0) else {
            panic!()
        };
        assert_eq!(r.seconds_left(0), 10);
        assert_eq!(r.seconds_left(9_000_001), 1);
        assert_eq!(r.seconds_left(10_000_000), 0);
    }
}
