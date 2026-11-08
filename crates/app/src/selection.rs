//! Device selection: auto-select a lone mouse, and "Detect" (next mouse that moves).

use analysis::DeviceKey;

/// Minimal device description the selection logic needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub key: DeviceKey,
    pub name: String,
    pub export_id: String,
}

#[derive(Debug, Default)]
pub struct Selection {
    devices: Vec<Device>,
    selected: Option<DeviceKey>,
    detecting: bool,
}

impl Selection {
    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn selected(&self) -> Option<&Device> {
        let key = self.selected?;
        self.devices.iter().find(|d| d.key == key)
    }

    pub fn selected_key(&self) -> Option<DeviceKey> {
        self.selected
    }

    pub fn is_detecting(&self) -> bool {
        self.detecting
    }

    /// Replaces the device list. Keeps the selection if the device is still
    /// present. A lone remaining device is selected automatically.
    pub fn set_devices(&mut self, devices: Vec<Device>) {
        self.devices = devices;
        if let Some(key) = self.selected
            && !self.devices.iter().any(|d| d.key == key)
        {
            self.selected = None;
        }
        if self.selected.is_none() && self.devices.len() == 1 {
            self.selected = Some(self.devices[0].key);
        }
    }

    /// Selects a device by key. Unknown keys are ignored.
    pub fn select(&mut self, key: DeviceKey) {
        if self.devices.iter().any(|d| d.key == key) {
            self.selected = Some(key);
            self.detecting = false;
        }
    }

    /// Starts "Detect": the next listed mouse that reports selects itself.
    pub fn start_detect(&mut self) {
        self.detecting = true;
    }

    pub fn cancel_detect(&mut self) {
        self.detecting = false;
    }

    /// Feed every device that produced a report. Returns true when detection
    /// picked a device.
    pub fn on_activity(&mut self, key: DeviceKey) -> bool {
        if self.detecting && self.devices.iter().any(|d| d.key == key) {
            self.select(key);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(k: u64) -> Device {
        Device {
            key: DeviceKey(k),
            name: format!("Mouse {k}"),
            export_id: format!("{k:012x}"),
        }
    }

    #[test]
    fn lone_mouse_is_selected() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1)]);
        assert_eq!(s.selected_key(), Some(DeviceKey(1)));
    }

    #[test]
    fn two_mice_need_a_choice() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1), dev(2)]);
        assert_eq!(s.selected_key(), None);
        s.select(DeviceKey(2));
        assert_eq!(s.selected().unwrap().name, "Mouse 2");
    }

    #[test]
    fn detect_picks_the_mouse_that_moves() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1), dev(2)]);
        assert!(!s.on_activity(DeviceKey(2)), "not detecting yet");
        s.start_detect();
        assert!(!s.on_activity(DeviceKey(99)), "unlisted device ignored");
        assert!(s.is_detecting());
        assert!(s.on_activity(DeviceKey(2)));
        assert_eq!(s.selected_key(), Some(DeviceKey(2)));
        assert!(!s.is_detecting());
    }

    #[test]
    fn removed_selection_is_cleared_or_falls_back() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1), dev(2), dev(3)]);
        s.select(DeviceKey(3));
        s.set_devices(vec![dev(1), dev(2)]);
        assert_eq!(s.selected_key(), None);
        s.set_devices(vec![dev(1)]);
        assert_eq!(s.selected_key(), Some(DeviceKey(1)));
    }

    #[test]
    fn selection_survives_refresh() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1), dev(2)]);
        s.select(DeviceKey(1));
        s.set_devices(vec![dev(2), dev(1), dev(3)]);
        assert_eq!(s.selected_key(), Some(DeviceKey(1)));
    }

    #[test]
    fn unknown_select_is_ignored() {
        let mut s = Selection::default();
        s.set_devices(vec![dev(1), dev(2)]);
        s.select(DeviceKey(5));
        assert_eq!(s.selected_key(), None);
    }
}
