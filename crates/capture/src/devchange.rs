//! Translation of `WM_INPUT_DEVICE_CHANGE` into [`DeviceChange`].

use analysis::DeviceKey;

/// `GIDC_ARRIVAL`
pub const GIDC_ARRIVAL: usize = 1;
/// `GIDC_REMOVAL`
pub const GIDC_REMOVAL: usize = 2;

/// A mouse was connected or disconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceChange {
    Arrived(DeviceKey),
    Removed(DeviceKey),
}

impl DeviceChange {
    pub fn device(self) -> DeviceKey {
        match self {
            DeviceChange::Arrived(d) | DeviceChange::Removed(d) => d,
        }
    }
}

/// Maps the message's `wParam` (change kind) and `lParam` (device handle).
/// Returns `None` for unknown kinds.
pub fn translate(wparam: usize, lparam: isize) -> Option<DeviceChange> {
    let key = DeviceKey(lparam as u64);
    match wparam {
        GIDC_ARRIVAL => Some(DeviceChange::Arrived(key)),
        GIDC_REMOVAL => Some(DeviceChange::Removed(key)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrival_and_removal() {
        assert_eq!(
            translate(1, 0x1_0045),
            Some(DeviceChange::Arrived(DeviceKey(0x1_0045)))
        );
        assert_eq!(
            translate(2, 0x1_0045),
            Some(DeviceChange::Removed(DeviceKey(0x1_0045)))
        );
        assert_eq!(translate(7, 1), None);
    }

    #[test]
    fn device_accessor() {
        assert_eq!(DeviceChange::Removed(DeviceKey(3)).device(), DeviceKey(3));
    }
}
