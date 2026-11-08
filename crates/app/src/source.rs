//! The capture source the UI drains every frame.
//!
//! On Windows this wraps [`capture::CaptureHandle`]. On other platforms there
//! is no capture. The type still exists so the UI type-checks during
//! development on macOS, but it never produces input.

use analysis::MouseEvent;
use capture::DeviceChange;

use crate::selection::Device;

pub struct Source {
    #[cfg(windows)]
    handle: capture::CaptureHandle,
}

impl Source {
    /// Starts capture. Fails with a user-facing message.
    pub fn start() -> Result<Self, String> {
        #[cfg(windows)]
        {
            capture::start()
                .map(|handle| Source { handle })
                .map_err(|e| format!("Mouse capture could not start: {e}"))
        }
        #[cfg(not(windows))]
        {
            Err("Mouse capture is only available on Windows.".to_owned())
        }
    }

    pub fn qpc_frequency(&self) -> u64 {
        #[cfg(windows)]
        return self.handle.qpc_frequency;
        #[cfg(not(windows))]
        1
    }

    pub fn now_ticks(&self) -> u64 {
        #[cfg(windows)]
        return self.handle.now_ticks();
        #[cfg(not(windows))]
        0
    }

    pub fn dropped_reports(&self) -> u64 {
        #[cfg(windows)]
        return self.handle.dropped_reports();
        #[cfg(not(windows))]
        0
    }

    /// Moves every queued report into `out`.
    pub fn drain(&mut self, out: &mut Vec<MouseEvent>) {
        #[cfg(windows)]
        while let Ok(e) = self.handle.events.pop() {
            out.push(e);
        }
        #[cfg(not(windows))]
        let _ = out;
    }

    /// Device changes since the last call.
    pub fn changes(&mut self) -> Vec<DeviceChange> {
        #[cfg(windows)]
        return self.handle.changes.try_iter().collect();
        #[cfg(not(windows))]
        Vec::new()
    }
}

/// Currently connected mice.
pub fn list_devices() -> Vec<Device> {
    #[cfg(windows)]
    {
        capture::list_mice()
            .unwrap_or_default()
            .into_iter()
            .map(|m| Device {
                key: m.key,
                name: m.name,
                export_id: m.export_id,
            })
            .collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}
