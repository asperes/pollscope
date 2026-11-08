//! Windows-only capture: device enumeration and the capture thread.
//!
//! # Safety invariants
//!
//! - Every Win32 call takes pointers only to locals or `Vec`s that outlive the
//!   call. Buffer sizes passed to the OS always equal the buffer's real
//!   capacity in bytes.
//! - The Raw Input buffer is a `Vec<u64>`, so it is 8-byte aligned as
//!   `GetRawInputBuffer` and `NEXTRAWINPUTBLOCK` require. Its contents are
//!   read through [`crate::rawbuf::walk`], which bounds-checks every block, never
//!   by casting to `RAWINPUT`.
//! - The message-only window and its class are created and destroyed on the
//!   capture thread. No `HWND` leaves that thread.
//! - Handles opened for name lookup are closed on every path.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use analysis::{DeviceKey, MouseEvent};
use windows::Win32::Devices::HumanInterfaceDevice::HidD_GetProductString;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
};
use windows::Win32::UI::Input::{
    GetRawInputBuffer, GetRawInputData, GetRawInputDeviceInfoW, GetRawInputDeviceList, HRAWINPUT,
    RAWINPUT, RAWINPUTDEVICE, RAWINPUTDEVICELIST, RAWINPUTHEADER, RID_INPUT, RIDEV_DEVNOTIFY,
    RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_DEVICENAME, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, HWND_MESSAGE, MSG,
    MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT,
    RegisterClassExW, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT,
    WM_INPUT_DEVICE_CHANGE, WM_QUIT, WNDCLASSEXW,
};
use windows::core::{PCWSTR, w};

use crate::devchange::{self, DeviceChange};
use crate::rawbuf;
use crate::{DeviceInfo, RING_CAPACITY, devpath};

/// Errors starting capture or enumerating devices.
#[derive(Debug)]
pub enum CaptureError {
    Win32(&'static str, windows::core::Error),
    ThreadDidNotStart,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::Win32(what, e) => write!(f, "{what} failed: {e}"),
            CaptureError::ThreadDidNotStart => write!(f, "the capture thread did not start"),
        }
    }
}

impl std::error::Error for CaptureError {}

fn last_error(what: &'static str) -> CaptureError {
    CaptureError::Win32(what, windows::core::Error::from_thread())
}

// ---------------------------------------------------------------- enumeration

/// Lists every Raw Input mouse currently connected.
pub fn list_mice() -> Result<Vec<DeviceInfo>, CaptureError> {
    let entry = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    let mut count = 0u32;
    // SAFETY: a null buffer asks only for the count, written to a local.
    if unsafe { GetRawInputDeviceList(None, &mut count, entry) } == u32::MAX {
        return Err(last_error("GetRawInputDeviceList (count)"));
    }
    let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
    // SAFETY: `list` holds `count` entries of `entry` bytes each.
    let got = unsafe { GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut count, entry) };
    if got == u32::MAX {
        return Err(last_error("GetRawInputDeviceList"));
    }
    list.truncate(got as usize);

    Ok(list
        .iter()
        .filter(|d| d.dwType == RIM_TYPEMOUSE)
        .map(|d| describe(d.hDevice))
        .collect())
}

fn describe(handle: HANDLE) -> DeviceInfo {
    let path = device_path(handle).unwrap_or_default();
    let name = product_string(&path).unwrap_or_else(|| devpath::fallback_name(&path));
    DeviceInfo {
        key: DeviceKey(handle.0 as u64),
        name,
        vid_pid: devpath::parse_vid_pid(&path),
        export_id: devpath::export_id(&path),
    }
}

/// The device interface path (`RIDI_DEVICENAME`).
fn device_path(handle: HANDLE) -> Option<String> {
    let mut chars = 0u32;
    // SAFETY: a null buffer asks only for the size, in characters.
    unsafe { GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut chars) };
    if chars == 0 {
        return None;
    }
    let mut buf = vec![0u16; chars as usize];
    // SAFETY: `buf` holds exactly `chars` UTF-16 units.
    let written = unsafe {
        GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &mut chars,
        )
    };
    if written == u32::MAX || written == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// HID product string, opened without read/write access so no exclusive
/// mouse handle is needed.
fn product_string(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let file = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .ok()?;
    let mut buf = [0u16; 127];
    // SAFETY: `file` is a valid handle and `buf` is 254 bytes, as passed.
    let ok = unsafe {
        HidD_GetProductString(
            file,
            buf.as_mut_ptr().cast::<c_void>(),
            std::mem::size_of_val(&buf) as u32,
        )
    };
    // SAFETY: `file` was opened above and is closed exactly once.
    let _ = unsafe { CloseHandle(file) };
    if !ok {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

// ------------------------------------------------------------------- capture

/// Running capture. Dropping it stops the thread.
pub struct CaptureHandle {
    /// Reports in arrival order.
    pub events: rtrb::Consumer<MouseEvent>,
    /// Device connect and disconnect notifications.
    pub changes: mpsc::Receiver<DeviceChange>,
    /// QPC ticks per second.
    pub qpc_frequency: u64,
    dropped: Arc<AtomicU64>,
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl CaptureHandle {
    /// Total reports dropped because the ring was full, since capture started.
    pub fn dropped_reports(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Reads the current QPC value (for run start and end markers).
    pub fn now_ticks(&self) -> u64 {
        qpc_now()
    }

    /// Stops the capture thread and waits for it to exit.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(join) = self.join.take() {
            // SAFETY: posting to a thread id has no memory-safety preconditions.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = join.join();
        }
    }
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn qpc_now() -> u64 {
    let mut t = 0i64;
    // SAFETY: writes one i64 to a local. Cannot fail on Windows XP and later.
    let _ = unsafe { QueryPerformanceCounter(&mut t) };
    t as u64
}

/// Starts the capture thread. Returns once it is registered for Raw Input.
pub fn start() -> Result<CaptureHandle, CaptureError> {
    let mut freq = 0i64;
    // SAFETY: writes one i64 to a local.
    unsafe { QueryPerformanceFrequency(&mut freq) }
        .map_err(|e| CaptureError::Win32("QueryPerformanceFrequency", e))?;

    let (producer, consumer) = rtrb::RingBuffer::new(RING_CAPACITY);
    let (change_tx, change_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, CaptureError>>();
    let dropped = Arc::new(AtomicU64::new(0));
    let dropped_thread = Arc::clone(&dropped);

    let join = std::thread::Builder::new()
        .name("raw-input-capture".into())
        .spawn(move || capture_thread(producer, change_tx, dropped_thread, ready_tx))
        .map_err(|_| CaptureError::ThreadDidNotStart)?;

    let thread_id = match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => {
            let _ = join.join();
            return Err(e);
        }
        Err(_) => return Err(CaptureError::ThreadDidNotStart),
    };

    Ok(CaptureHandle {
        events: consumer,
        changes: change_rx,
        qpc_frequency: freq as u64,
        dropped,
        thread_id,
        join: Some(join),
    })
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the OS-supplied arguments unchanged.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

const CLASS_NAME: PCWSTR = w!("RawInputCaptureSink");

fn capture_thread(
    mut producer: rtrb::Producer<MouseEvent>,
    changes: mpsc::Sender<DeviceChange>,
    dropped: Arc<AtomicU64>,
    ready: mpsc::Sender<Result<u32, CaptureError>>,
) {
    // SAFETY: changes only this thread's priority.
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL) };

    let hwnd = match create_sink_window() {
        Ok(h) => h,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    if let Err(e) = register(hwnd, false) {
        destroy_sink_window(hwnd);
        let _ = ready.send(Err(e));
        return;
    }
    // SAFETY: no preconditions.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    // 64 KiB, 8-byte aligned (see module safety invariants).
    let mut buffer = vec![0u64; 8192];
    let mut batch_id = 0u64;
    let header = std::mem::size_of::<RAWINPUTHEADER>() as u32;

    'outer: loop {
        // Sleep until raw input or any message is queued.
        // SAFETY: no handles are passed.
        unsafe { MsgWaitForMultipleObjects(None, false, u32::MAX, QS_ALLINPUT) };

        // Drain all pending raw input. Each successful read is one batch.
        loop {
            let mut size = (buffer.len() * 8) as u32;
            // SAFETY: `buffer` is `size` bytes, 8-byte aligned and outlives the call.
            let count = unsafe {
                GetRawInputBuffer(
                    Some(buffer.as_mut_ptr().cast::<RAWINPUT>()),
                    &mut size,
                    header,
                )
            };
            if count == 0 || count == u32::MAX {
                break;
            }
            let ticks = qpc_now();
            push_reports(
                as_bytes(&buffer),
                count,
                ticks,
                batch_id,
                &mut producer,
                &dropped,
            );
            batch_id += 1;
        }

        // Leftover WM_INPUT messages that GetRawInputBuffer did not consume:
        // read each one's report as its own batch, then let DefWindowProc
        // release it, so the queue never fills up.
        let mut msg = MSG::default();
        // SAFETY: `msg` is a local MSG.
        while unsafe { PeekMessageW(&mut msg, None, WM_INPUT, WM_INPUT, PM_REMOVE) }.as_bool() {
            let mut size = (buffer.len() * 8) as u32;
            // SAFETY: `lParam` is the HRAWINPUT of a WM_INPUT this thread just
            // removed; `buffer` is `size` bytes and 8-byte aligned.
            let got = unsafe {
                GetRawInputData(
                    HRAWINPUT(msg.lParam.0 as *mut c_void),
                    RID_INPUT,
                    Some(buffer.as_mut_ptr().cast::<c_void>()),
                    &mut size,
                    header,
                )
            };
            if got != 0 && got != u32::MAX {
                let ticks = qpc_now();
                push_reports(
                    as_bytes(&buffer),
                    1,
                    ticks,
                    batch_id,
                    &mut producer,
                    &dropped,
                );
                batch_id += 1;
            }
            // SAFETY: forwards the removed message so the OS frees its data.
            unsafe { DefWindowProcW(msg.hwnd, msg.message, msg.wParam, msg.lParam) };
        }

        // Every other message.
        for (lo, hi) in [(0, WM_INPUT - 1), (WM_INPUT + 1, u32::MAX)] {
            // SAFETY: `msg` is a local MSG.
            while unsafe { PeekMessageW(&mut msg, None, lo, hi, PM_REMOVE) }.as_bool() {
                match msg.message {
                    WM_QUIT => break 'outer,
                    WM_INPUT_DEVICE_CHANGE => {
                        if let Some(c) = devchange::translate(msg.wParam.0, msg.lParam.0) {
                            let _ = changes.send(c);
                        }
                    }
                    _ => {
                        // SAFETY: dispatching a message this thread received.
                        unsafe { DispatchMessageW(&msg) };
                    }
                }
            }
        }
    }

    let _ = register(hwnd, true);
    destroy_sink_window(hwnd);
}

/// Decodes `count` blocks and pushes them as one batch. A full ring drops and counts.
fn push_reports(
    bytes: &[u8],
    count: u32,
    ticks: u64,
    batch_id: u64,
    producer: &mut rtrb::Producer<MouseEvent>,
    dropped: &AtomicU64,
) {
    let _ = rawbuf::walk(bytes, count, |r| {
        let ev = MouseEvent {
            device: DeviceKey(r.device),
            ticks,
            batch_id,
            dx: r.last_x,
            dy: r.last_y,
            buttons: r.button_flags,
        };
        if producer.push(ev).is_err() {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    });
}

fn as_bytes(buf: &[u64]) -> &[u8] {
    // SAFETY: u64 has no padding or invalid bit patterns as bytes, and the
    // byte slice covers exactly the same allocation with the same lifetime.
    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), std::mem::size_of_val(buf)) }
}

fn create_sink_window() -> Result<HWND, CaptureError> {
    // SAFETY: `None` returns the current module's handle.
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|e| CaptureError::Win32("GetModuleHandleW", e))?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: module.into(),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised. Registering twice (after an
    // earlier stop) fails harmlessly with ERROR_CLASS_ALREADY_EXISTS.
    unsafe { RegisterClassExW(&class) };
    // SAFETY: the class name is static; a message-only window needs no other resources.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(|e| CaptureError::Win32("CreateWindowExW", e))
}

fn destroy_sink_window(hwnd: HWND) {
    // SAFETY: `hwnd` was created on this thread and is destroyed once.
    let _ = unsafe { DestroyWindow(hwnd) };
    // SAFETY: the module handle comes from the OS; failure is harmless.
    if let Ok(module) = unsafe { GetModuleHandleW(None) } {
        // SAFETY: unregistering a class with no remaining windows.
        let _ = unsafe { UnregisterClassW(CLASS_NAME, Some(module.into())) };
    }
}

/// Registers (or with `remove`, unregisters) mouse Raw Input for `hwnd`.
fn register(hwnd: HWND, remove: bool) -> Result<(), CaptureError> {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: if remove {
            RIDEV_REMOVE
        } else {
            RIDEV_INPUTSINK | RIDEV_DEVNOTIFY
        },
        hwndTarget: if remove { HWND::default() } else { hwnd },
    };
    // SAFETY: one fully initialised RAWINPUTDEVICE, with its size.
    unsafe { RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32) }
        .map_err(|e| CaptureError::Win32("RegisterRawInputDevices", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn enumeration_succeeds() {
        // CI runners may have no mouse; enumeration must still succeed.
        let mice = list_mice().expect("list_mice");
        for m in &mice {
            assert!(!m.name.is_empty());
            assert_eq!(m.export_id.len(), 12);
        }
    }

    #[test]
    fn capture_starts_and_stops_quickly() {
        let t = Instant::now();
        let handle = start().expect("start capture");
        assert!(handle.qpc_frequency > 0);
        assert_eq!(handle.dropped_reports(), 0);
        handle.stop();
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    }

    #[test]
    fn capture_can_restart() {
        start().expect("first").stop();
        start().expect("second").stop();
    }

    #[test]
    fn qpc_is_monotonic() {
        let a = qpc_now();
        let b = qpc_now();
        assert!(b >= a);
    }
}
