// The trait methods take the C ABI's raw pointers and check them before use;
// the exported `extern "C"` wrappers are what C calls, so they cannot be
// `unsafe fn`. Same allowance as `nros-platform-cffi`'s.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::{CStr, c_char, c_void};
use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard},
};

use nros_platform_api::PlatformCan;
pub use nros_platform_cffi::nros_platform_can_frame_t as Frame;
use nros_platform_cffi::{
    NROS_PLATFORM_CAN_EAGAIN, NROS_PLATFORM_CAN_EINVAL, NROS_PLATFORM_CAN_ENOFD,
    NROS_PLATFORM_CAN_ENOTOPEN, NROS_PLATFORM_CAN_OK, NROS_PLATFORM_CAN_OPAQUE_U64S,
    nros_platform_can_t,
};

unsafe extern "C" {
    // `c/frame_check.c`: the header's `static inline` frame check, exported.
    fn nros_can_mock_frame_check(frame: *const Frame, fd_active: u8) -> i8;
}

/// "NCMK": the state of an open mock handle. A zeroed handle is closed.
const OPEN_MAGIC: u32 = 0x4e43_4d4b;

/// What the mock keeps in the caller's `nros_platform_can_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct HandleState {
    magic: u32,
    fd_active: u8,
}

// RFC-0132 D2a: the backend proves its state fits the handle the header sizes.
// Measured: 8 bytes, alignment 4 (x86_64), against 128 bytes, alignment 8.
const _: () = assert!(
    core::mem::size_of::<HandleState>() <= NROS_PLATFORM_CAN_OPAQUE_U64S as usize * 8
        && core::mem::align_of::<HandleState>() <= core::mem::align_of::<nros_platform_can_t>(),
    "NROS_PLATFORM_CAN_OPAQUE_U64S too small for nros-can-mock's HandleState"
);

/// The measured size of the mock's handle state, in bytes (recorded beside
/// `NROS_PLATFORM_CAN_OPAQUE_U64S` in the header).
pub const HANDLE_STATE_SIZE: usize = core::mem::size_of::<HandleState>();

struct Bus {
    fd_capable: bool,
    sent: Vec<Frame>,
    rx: VecDeque<Frame>,
    opened: Option<Option<String>>,
}

static BUS: Mutex<Bus> = Mutex::new(Bus {
    fd_capable: true,
    sent: Vec::new(),
    rx: VecDeque::new(),
    opened: None,
});

fn bus() -> MutexGuard<'static, Bus> {
    // A test that panicked while holding the bus leaves plain data behind;
    // the next test resets it, so poisoning carries no information here.
    BUS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Empty the bus and make the device FD-capable again.
pub fn reset() {
    let mut b = bus();
    b.fd_capable = true;
    b.sent.clear();
    b.rx.clear();
    b.opened = None;
}

/// Whether the mock device carries CAN FD (default `true`). With `false`,
/// `open(.., want_fd = 1)` fails with `NROS_PLATFORM_CAN_ENOFD`, as it does on a
/// classic-only device.
pub fn set_fd_capable(capable: bool) {
    bus().fd_capable = capable;
}

/// Take every frame sent since the last call (or [`reset`]), oldest first.
pub fn take_sent() -> Vec<Frame> {
    core::mem::take(&mut bus().sent)
}

/// Queue a frame for `recv` to return. Queued frames are returned as given.
pub fn push_rx(frame: Frame) {
    bus().rx.push_back(frame);
}

/// The device the last successful `open` named: `None` before any open,
/// `Some(None)` for `open(NULL, ..)` (the image's device), `Some(Some(name))`
/// for a named one.
pub fn last_opened_device() -> Option<Option<String>> {
    bus().opened.clone()
}

fn state(can: *const c_void) -> Option<HandleState> {
    if can.is_null() {
        return None;
    }
    // SAFETY: `can` points at a caller's `nros_platform_can_t` (128 bytes,
    // 8-aligned, the assert above), which holds at least a `HandleState`.
    let s = unsafe { (can as *const HandleState).read() };
    (s.magic == OPEN_MAGIC).then_some(s)
}

/// The mock as a `PlatformCan` implementer; [`nros_platform_cffi::nros_platform_export_can!`]
/// turns it into the five C symbols.
pub struct MockCan;

impl PlatformCan for MockCan {
    fn open(can: *mut c_void, device: *const c_char, want_fd: u8) -> i8 {
        if can.is_null() {
            return NROS_PLATFORM_CAN_EINVAL as i8;
        }
        let mut b = bus();
        if want_fd != 0 && !b.fd_capable {
            return NROS_PLATFORM_CAN_ENOFD as i8;
        }
        let name = if device.is_null() {
            None
        } else {
            // SAFETY: a non-NULL `device` is a NUL-terminated string (the ABI).
            Some(
                unsafe { CStr::from_ptr(device) }
                    .to_string_lossy()
                    .into_owned(),
            )
        };
        // SAFETY: `can` is the caller's handle storage, large and aligned
        // enough for `HandleState` (the assert above).
        unsafe {
            (can as *mut HandleState).write(HandleState {
                magic: OPEN_MAGIC,
                fd_active: u8::from(want_fd != 0),
            });
        }
        b.opened = Some(name);
        NROS_PLATFORM_CAN_OK as i8
    }

    fn send(can: *mut c_void, frame: *const c_void, _timeout_us: u32) -> i8 {
        let Some(s) = state(can) else {
            return NROS_PLATFORM_CAN_ENOTOPEN as i8;
        };
        let frame = frame as *const Frame;
        // SAFETY: the shim reads `*frame` only when it is non-NULL.
        let rc = unsafe { nros_can_mock_frame_check(frame, s.fd_active) };
        if rc != NROS_PLATFORM_CAN_OK as i8 {
            return rc;
        }
        // SAFETY: checked non-NULL by the frame check above.
        let f = unsafe { *frame };
        // The bus records synchronously, so the frame has "left the
        // controller" when this returns: NROS_PLATFORM_CAN_SEND_WAIT_TX holds.
        bus().sent.push(f);
        NROS_PLATFORM_CAN_OK as i8
    }

    fn fd_active(can: *const c_void) -> u8 {
        state(can).map_or(0, |s| s.fd_active)
    }

    fn recv(can: *mut c_void, frame: *mut c_void, _timeout_us: u32) -> i8 {
        if state(can).is_none() {
            return NROS_PLATFORM_CAN_ENOTOPEN as i8;
        }
        if frame.is_null() {
            return NROS_PLATFORM_CAN_EINVAL as i8;
        }
        // The mock never blocks: an empty queue is "nothing arrived" at once.
        match bus().rx.pop_front() {
            Some(f) => {
                // SAFETY: non-NULL, and the ABI makes it a frame's storage.
                unsafe { (frame as *mut Frame).write(f) };
                NROS_PLATFORM_CAN_OK as i8
            }
            None => NROS_PLATFORM_CAN_EAGAIN as i8,
        }
    }

    fn close(can: *mut c_void) {
        if can.is_null() {
            return;
        }
        // SAFETY: the caller's handle storage; zeroed is the closed state.
        unsafe {
            (can as *mut HandleState).write(HandleState {
                magic: 0,
                fd_active: 0,
            })
        };
    }
}

nros_platform_cffi::nros_platform_export_can!(MockCan);
