//! Walks the byte buffer filled by `GetRawInputBuffer` on 64-bit Windows.
//!
//! Parsing works on plain bytes with explicit x64 offsets rather than casting
//! to `RAWINPUT`, so it is safe Rust and testable on any OS against byte
//! fixtures. Layout (x86_64):
//!
//! ```text
//! RAWINPUTHEADER  (24 bytes)  dwType u32 @0, dwSize u32 @4, hDevice u64 @8, wParam u64 @16
//! RAWMOUSE        (24 bytes)  usFlags u16 @24, usButtonFlags u16 @28, usButtonData u16 @30,
//!                             ulRawButtons u32 @32, lLastX i32 @36, lLastY i32 @40, ulExtra u32 @44
//! ```
//!
//! Blocks follow one another at `NEXTRAWINPUTBLOCK`: the previous block's start
//! plus its `dwSize`, rounded up to 8 bytes.

/// `RIM_TYPEMOUSE`
pub const RIM_TYPEMOUSE: u32 = 0;
/// `sizeof(RAWINPUTHEADER)` on x64.
pub const HEADER_SIZE: usize = 24;
/// `sizeof(RAWINPUTHEADER) + sizeof(RAWMOUSE)` on x64.
pub const MOUSE_BLOCK_SIZE: usize = 48;

/// One mouse report decoded from the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawMouseRecord {
    /// `hDevice`, the session-local device handle (0 for injected input).
    pub device: u64,
    /// `usFlags` (bit 0 set = absolute coordinates).
    pub flags: u16,
    pub button_flags: u16,
    pub last_x: i32,
    pub last_y: i32,
}

/// Malformed buffer contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkError {
    /// A block's `dwSize` is smaller than a header or runs past the buffer.
    BadBlockSize { offset: usize, size: u32 },
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn i32_at(b: &[u8], o: usize) -> i32 {
    u32_at(b, o) as i32
}
fn u64_at(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}

/// Decodes `count` blocks from `buf`, calling `sink` for each mouse block.
/// Non-mouse blocks are skipped. Returns the number of mouse blocks.
pub fn walk(
    buf: &[u8],
    count: u32,
    mut sink: impl FnMut(RawMouseRecord),
) -> Result<u32, WalkError> {
    let mut offset = 0usize;
    let mut mice = 0;
    for _ in 0..count {
        if offset + HEADER_SIZE > buf.len() {
            return Err(WalkError::BadBlockSize { offset, size: 0 });
        }
        let kind = u32_at(buf, offset);
        let size = u32_at(buf, offset + 4);
        let end = offset + size as usize;
        if (size as usize) < HEADER_SIZE || end > buf.len() {
            return Err(WalkError::BadBlockSize { offset, size });
        }
        if kind == RIM_TYPEMOUSE {
            if (size as usize) < MOUSE_BLOCK_SIZE {
                return Err(WalkError::BadBlockSize { offset, size });
            }
            let b = &buf[offset..end];
            sink(RawMouseRecord {
                device: u64_at(b, 8),
                flags: u16_at(b, 24),
                button_flags: u16_at(b, 28),
                last_x: i32_at(b, 36),
                last_y: i32_at(b, 40),
            });
            mice += 1;
        }
        offset = end.next_multiple_of(8);
    }
    Ok(mice)
}

/// Encodes one mouse block in the x64 layout (test and fixture helper).
pub fn encode_mouse_block(rec: &RawMouseRecord) -> [u8; MOUSE_BLOCK_SIZE] {
    let mut b = [0u8; MOUSE_BLOCK_SIZE];
    b[0..4].copy_from_slice(&RIM_TYPEMOUSE.to_le_bytes());
    b[4..8].copy_from_slice(&(MOUSE_BLOCK_SIZE as u32).to_le_bytes());
    b[8..16].copy_from_slice(&rec.device.to_le_bytes());
    b[24..26].copy_from_slice(&rec.flags.to_le_bytes());
    b[28..30].copy_from_slice(&rec.button_flags.to_le_bytes());
    b[36..40].copy_from_slice(&rec.last_x.to_le_bytes());
    b[40..44].copy_from_slice(&rec.last_y.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(device: u64, x: i32, y: i32) -> RawMouseRecord {
        RawMouseRecord {
            device,
            flags: 0,
            button_flags: 0,
            last_x: x,
            last_y: y,
        }
    }

    #[test]
    fn multi_report_batch() {
        let recs = [
            rec(0x1_0045, 3, -1),
            rec(0x1_0045, 2, 0),
            rec(0x2_0099, -7, 5),
        ];
        let mut buf = Vec::new();
        for r in &recs {
            buf.extend_from_slice(&encode_mouse_block(r));
        }
        let mut out = Vec::new();
        let n = walk(&buf, 3, |r| out.push(r)).unwrap();
        assert_eq!(n, 3);
        assert_eq!(out, recs);
    }

    #[test]
    fn button_flags_and_negative_motion() {
        let r = RawMouseRecord {
            device: 9,
            flags: 0,
            button_flags: 0x0001,
            last_x: -32768,
            last_y: i32::MIN,
        };
        let mut out = Vec::new();
        walk(&encode_mouse_block(&r), 1, |x| out.push(x)).unwrap();
        assert_eq!(out, vec![r]);
    }

    #[test]
    fn skips_non_mouse_blocks_and_honours_alignment() {
        // A 30-byte keyboard-like block (type 1) padded to 32, then a mouse block.
        let mut buf = vec![0u8; 32];
        buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        buf[4..8].copy_from_slice(&30u32.to_le_bytes());
        buf.extend_from_slice(&encode_mouse_block(&rec(5, 1, 1)));
        let mut out = Vec::new();
        let n = walk(&buf, 2, |r| out.push(r)).unwrap();
        assert_eq!(n, 1);
        assert_eq!(out, vec![rec(5, 1, 1)]);
    }

    #[test]
    fn rejects_truncated_buffer() {
        let block = encode_mouse_block(&rec(1, 0, 0));
        let err = walk(&block[..40], 1, |_| {}).unwrap_err();
        assert_eq!(
            err,
            WalkError::BadBlockSize {
                offset: 0,
                size: 48
            }
        );
        assert!(walk(&block, 2, |_| {}).is_err());
    }

    #[test]
    fn rejects_zero_size_block() {
        let mut block = encode_mouse_block(&rec(1, 0, 0));
        block[4..8].copy_from_slice(&0u32.to_le_bytes());
        assert!(walk(&block, 1, |_| {}).is_err());
    }
}
