//! Device interface path helpers: VID/PID parsing, fallback names, export ids.

use sha2::{Digest, Sha256};

/// Vendor and product id parsed from a device interface path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VidPid {
    pub vendor_id: u16,
    pub product_id: u16,
}

/// Parses `VID_xxxx` and `PID_xxxx` (case-insensitive) from an interface path
/// such as `\\?\HID#VID_046D&PID_C539&MI_01&Col01#8&1d2f3a&0&0000#{...}`.
/// Bluetooth paths using `VID&0002046d_PID&b023` are handled too.
pub fn parse_vid_pid(path: &str) -> Option<VidPid> {
    let upper = path.to_ascii_uppercase();
    let vendor_id = hex_after(&upper, "VID_").or_else(|| bt_hex_after(&upper, "VID&"))?;
    let product_id = hex_after(&upper, "PID_").or_else(|| bt_hex_after(&upper, "PID&"))?;
    Some(VidPid {
        vendor_id,
        product_id,
    })
}

fn hex_after(s: &str, key: &str) -> Option<u16> {
    let start = s.find(key)? + key.len();
    let digits = s.get(start..start + 4)?;
    u16::from_str_radix(digits, 16).ok()
}

/// Bluetooth LE form: `VID&02046D` (2-digit source + 4-digit id) or `VID&0002046D`.
fn bt_hex_after(s: &str, key: &str) -> Option<u16> {
    let start = s.find(key)? + key.len();
    let rest = &s[start..];
    let len = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
    if len < 4 {
        return None;
    }
    u16::from_str_radix(&rest[len - 4..len], 16).ok()
}

/// Display name when Windows provides no product string.
pub fn fallback_name(path: &str) -> String {
    match parse_vid_pid(path) {
        Some(v) => format!("Mouse VID_{:04X} PID_{:04X}", v.vendor_id, v.product_id),
        None => "Mouse (unknown vendor)".to_owned(),
    }
}

/// Identifier written to exports: the first 12 hex digits of SHA-256 of the
/// lower-cased interface path. Stable for the same device and port, but it
/// doesn't reveal the instance path.
pub fn export_id(path: &str) -> String {
    let digest = Sha256::digest(path.to_lowercase().as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_hid_path() {
        let p = r"\\?\HID#VID_046D&PID_C539&MI_01&Col01#8&1d2f3a&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(
            parse_vid_pid(p),
            Some(VidPid {
                vendor_id: 0x046D,
                product_id: 0xC539
            })
        );
        assert_eq!(fallback_name(p), "Mouse VID_046D PID_C539");
    }

    #[test]
    fn lower_case_path() {
        let p =
            r"\\?\hid#vid_1532&pid_00b6&mi_00#7&abc&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(
            parse_vid_pid(p),
            Some(VidPid {
                vendor_id: 0x1532,
                product_id: 0x00B6
            })
        );
    }

    #[test]
    fn bluetooth_le_path() {
        let p = r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b023_REV&0011#9&2a&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(
            parse_vid_pid(p),
            Some(VidPid {
                vendor_id: 0x046D,
                product_id: 0xB023
            })
        );
    }

    #[test]
    fn rdp_or_unknown_path() {
        let p = r"\\?\Root#RDP_MOU#0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}";
        assert_eq!(parse_vid_pid(p), None);
        assert_eq!(fallback_name(p), "Mouse (unknown vendor)");
    }

    #[test]
    fn export_id_is_short_stable_and_case_insensitive() {
        let a = export_id(r"\\?\HID#VID_046D&PID_C539#x");
        let b = export_id(r"\\?\hid#vid_046d&pid_c539#X");
        assert_eq!(a, b);
        assert_eq!(a.len(), 12);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(!a.contains("046"));
    }
}
