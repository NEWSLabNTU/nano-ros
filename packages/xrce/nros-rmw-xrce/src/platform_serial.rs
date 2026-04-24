//! Platform-agnostic serial transport for XRCE-DDS.
//!
//! XRCE's `uxrCustomTransport` callbacks route through whatever platform
//! implements [`nros_platform_api::PlatformSerial`]. On POSIX that's
//! `nros-platform-posix`'s `serial` module (termios + poll); bare-metal
//! platforms that need serial XRCE implement the trait via their UART
//! peripheral.
//!
//! The single-session discipline is enforced here, not in
//! `PlatformSerial`: XRCE opens exactly one device per `XrceSession`,
//! we cache the returned handle in a module-level `SerialCell`, and
//! the transport callbacks consume the cached handle on every
//! invocation. Multi-device platforms can still be reused from other
//! callers (`zpico-serial` et al.) without interference.

use core::cell::UnsafeCell;
use core::ffi::c_int;

use nros_platform::{ConcretePlatform, PlatformSerial};

type P = ConcretePlatform;
type Handle = <P as PlatformSerial>::Handle;

// ============================================================================
// Single-session handle cache
// ============================================================================
//
// `init_platform_serial_transport(path)` opens the device and caches
// the resulting handle here. The four `uxrCustomTransport` callbacks
// below pull the handle back out on every invocation.
//
// Access is single-threaded — XRCE is single-session and the
// transport callbacks run inside the session's fetch/dispatch loop.
// The `SharedCell` pattern mirrors `nros-rmw-xrce::lib`'s `STATE`.

#[repr(transparent)]
struct SharedCell<T>(UnsafeCell<T>);
// SAFETY: access is single-threaded (see above).
unsafe impl<T> Sync for SharedCell<T> {}

impl<T> SharedCell<T> {
    const fn new(t: T) -> Self {
        Self(UnsafeCell::new(t))
    }
    #[inline(always)]
    unsafe fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

// `Handle` isn't known to be `Copy` at `const` init time from the
// trait's POV — we can only express `Self::INVALID` inside a method
// body. Use `MaybeUninit` for static init and initialise on first
// `init_platform_serial_transport` call.
use core::mem::MaybeUninit;

static CACHED_HANDLE: SharedCell<MaybeUninit<Handle>> = SharedCell::new(MaybeUninit::uninit());
static DEVICE_PATH: SharedCell<[u8; DEVICE_PATH_BUF_SIZE]> =
    SharedCell::new([0u8; DEVICE_PATH_BUF_SIZE]);

/// Stack buffer size for device path (including null terminator).
const DEVICE_PATH_BUF_SIZE: usize = 256;

#[allow(static_mut_refs)]
fn cached_handle() -> Handle {
    // SAFETY: only read after `init_platform_serial_transport` has run
    // and populated the cell. If XRCE called a transport callback
    // before init, `P::INVALID` signals "no live device" and the
    // platform-side implementation errors cleanly.
    unsafe {
        let cell = CACHED_HANDLE.get_mut();
        // Treat uninit as INVALID — safe only because `P::Handle: Copy`
        // and the pointed-to storage is always assumed-initialised after
        // the first call (which is the only caller pattern XRCE uses).
        core::ptr::read(cell.as_ptr())
    }
}

fn set_cached_handle(h: Handle) {
    unsafe {
        CACHED_HANDLE.get_mut().write(h);
    }
}

// ============================================================================
// XRCE custom transport callbacks
// ============================================================================

unsafe extern "C" fn serial_transport_open(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    // Re-open from the cached path. XRCE calls `open` again after a
    // session reset, so we use the path the user supplied at
    // `init_platform_serial_transport` time.
    let path_ptr = unsafe { DEVICE_PATH.get_mut().as_ptr() };
    let h = <P as PlatformSerial>::open(path_ptr);
    if !<P as PlatformSerial>::is_valid(h) {
        return false;
    }
    set_cached_handle(h);
    true
}

unsafe extern "C" fn serial_transport_close(_transport: *mut xrce_sys::uxrCustomTransport) -> bool {
    let h = cached_handle();
    if <P as PlatformSerial>::is_valid(h) {
        <P as PlatformSerial>::close(h);
        set_cached_handle(<P as PlatformSerial>::INVALID);
    }
    true
}

unsafe extern "C" fn serial_transport_write(
    _transport: *mut xrce_sys::uxrCustomTransport,
    buffer: *const u8,
    length: usize,
    error_code: *mut u8,
) -> usize {
    let h = cached_handle();
    let n = <P as PlatformSerial>::write(h, buffer, length);
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
    let h = cached_handle();
    let timeout_ms = if timeout <= 0 { 0 } else { timeout as u32 };
    let n = <P as PlatformSerial>::read(h, buffer, length, timeout_ms);
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
/// Not thread-safe: only one XRCE serial session may be active per
/// process. The cached handle is single-slot; reinitialising from a
/// second thread would race.
#[allow(static_mut_refs)]
pub unsafe fn init_platform_serial_transport(device_path: &str) {
    unsafe {
        // Copy the path into our static buffer so the transport-side
        // `open` callback can re-use it if XRCE tears down and
        // re-establishes the session.
        let path_buf = DEVICE_PATH.get_mut();
        let len = device_path.len().min(DEVICE_PATH_BUF_SIZE - 1);
        path_buf[..len].copy_from_slice(&device_path.as_bytes()[..len]);
        path_buf[len] = 0;

        // Start with an invalid-handle sentinel; the transport-side
        // `open` callback fills it in.
        set_cached_handle(<P as PlatformSerial>::INVALID);

        crate::init_transport(
            Some(serial_transport_open),
            Some(serial_transport_close),
            Some(serial_transport_write),
            Some(serial_transport_read),
            true, // serial is byte-stream — needs HDLC framing
        );
    }
}
