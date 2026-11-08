//! Windows Raw Input mouse capture.
//!
//! [`start`] spawns a dedicated capture thread that owns a hidden message-only
//! window registered for mouse Raw Input. That thread timestamps each read with
//! the QPC and pushes [`analysis::MouseEvent`]s into a lock-free ring, which the
//! UI drains. The capture thread never blocks on the UI. If the ring is full it
//! counts the dropped reports instead.
//!
//! The parsing and translation modules are plain safe Rust and are unit-tested
//! on every platform. Only [`win`] (and so [`start`] and [`list_mice`]) exists
//! on Windows.

pub mod devchange;
pub mod devpath;
pub mod rawbuf;

#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub use win::{CaptureError, CaptureHandle, list_mice, start};

pub use devchange::DeviceChange;

/// A connected mouse as shown in the device picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Session-local key, the same value as [`analysis::MouseEvent::device`].
    pub key: analysis::DeviceKey,
    /// Product string, or a VID/PID fallback.
    pub name: String,
    pub vid_pid: Option<devpath::VidPid>,
    /// Hashed identifier safe to export (see [`devpath::export_id`]).
    pub export_id: String,
}

/// Capacity of the capture → UI ring: about 8 s of 8 kHz input.
pub const RING_CAPACITY: usize = 65_536;
