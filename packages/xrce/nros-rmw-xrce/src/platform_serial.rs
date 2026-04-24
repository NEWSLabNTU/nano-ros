//! Platform-agnostic serial transport for XRCE-DDS.
//!
//! XRCE's `uxrCustomTransport` callbacks route through whatever platform
//! implements [`nros_platform_api::PlatformSerial`]. On POSIX that's
//! `nros-platform-posix`'s `serial` module (termios + poll); bare-metal
//! platforms that need serial XRCE implement the trait via their UART
//! peripheral.
//!
//! Replaces the hand-rolled `posix_serial.rs` from Phase 80.14.

use core::ffi::c_int;

use nros_platform::{ConcretePlatform, PlatformSerial};

type P = ConcretePlatform;

// ============================================================================
// XRCE custom transport callbacks
// ============================================================================

unsafe extern "C" fn serial_transport_open(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    // The caller of `init_platform_serial_transport` stored the device
    // path via `<P as PlatformSerial>::open(path)` before XRCE was
    // started, so the session-layer open callback just re-invokes open()
    // to get the live FD / handle.
    <P as PlatformSerial>::open(device_path().as_ptr()) == 0
}

unsafe extern "C" fn serial_transport_close(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    <P as PlatformSerial>::close();
    true
}

unsafe extern "C" fn serial_transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    let n = <P as PlatformSerial>::write(buffer, length);
    if n == usize::MAX {
        unsafe { *error_code = 1 };
        0
    } else {
        n
    }
}

unsafe extern "C" fn serial_transport_read(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *mut u8,
    length: usize,
    timeout: c_int,
    error_code: *mut u8,
) -> usize {
    let timeout_ms = if timeout <= 0 { 0 } else { timeout as u32 };
    let n = <P as PlatformSerial>::read(buffer, length, timeout_ms);
    if n == usize::MAX {
        unsafe { *error_code = 1 };
        0
    } else {
        n
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Stack buffer size for device path (including null terminator).
const DEVICE_PATH_BUF_SIZE: usize = 256;

static mut DEVICE_PATH: [u8; DEVICE_PATH_BUF_SIZE] = [0u8; DEVICE_PATH_BUF_SIZE];
static mut DEVICE_PATH_LEN: usize = 0;

#[allow(static_mut_refs)]
fn device_path() -> &'static [u8] {
    unsafe { &DEVICE_PATH[..=DEVICE_PATH_LEN] }
}

/// Register XRCE serial-transport callbacks routed through the active
/// [`PlatformSerial`] implementation, and open the device.
///
/// `device_path` is platform-specific — on POSIX it's a PTY / `/dev/tty*`
/// path; on bare-metal it's a port identifier parsed by the board's
/// [`PlatformSerial::open`] impl.
///
/// Must be called before [`crate::XrceRmw::open()`].
///
/// # Safety
///
/// Not thread-safe: only one serial transport may be active per process,
/// matching the single-session model of both XRCE and the underlying
/// platform serial impl.
#[allow(static_mut_refs)]
pub unsafe fn init_platform_serial_transport(device_path: &str) {
    unsafe {
        let len = device_path.len().min(DEVICE_PATH_BUF_SIZE - 1);
        DEVICE_PATH[..len].copy_from_slice(&device_path.as_bytes()[..len]);
        DEVICE_PATH[len] = 0;
        DEVICE_PATH_LEN = len;

        crate::init_transport(
            Some(serial_transport_open),
            Some(serial_transport_close),
            Some(serial_transport_write),
            Some(serial_transport_read),
            true, // serial is byte-stream — needs HDLC framing
        );
    }
}
