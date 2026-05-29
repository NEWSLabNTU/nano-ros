//! Phase 207.2 — XRCE custom-transport callbacks for the CMSDK UART0.
//!
//! Wraps the same `UART_DEVICE` (`cmsdk_uart::CmsdkUart`) the zenoh-pico
//! serial path uses, exposing the four `open`/`close`/`write`/`read`
//! callbacks the XRCE custom-transport profile expects. Gated by the
//! `xrce-transport` Cargo feature so the symbol set only enters the
//! build when an XRCE example asks for it (no overhead on the default
//! zenoh-pico path).
//!
//! # Use (from an XRCE bare-metal example)
//!
//! ```ignore
//! use nros_board_mps2_an385::xrce_transport;
//! use nros_rmw_xrce_cffi as xrce;
//!
//! // Install BEFORE Executor::open. Framing = true for UART (HDLC).
//! let ops = xrce_transport::xrce_transport_ops();
//! unsafe { xrce::set_custom_transport_ops(&ops, true).expect("install"); }
//! xrce::register().expect("register");
//! // … now `Executor::open` routes XRCE I/O through UART0.
//! ```

use core::ffi::c_void;

use zpico_serial::SerialPort;

use crate::node::UART_DEVICE;

/// XRCE `open`. The UART is enabled by the board crate during
/// `init_serial`, so this is a no-op that returns success (0). XRCE
/// invokes this once per session start.
unsafe extern "C" fn xrce_open(_user_data: *mut c_void, _params: *const c_void) -> i32 {
    0
}

/// XRCE `close`. The UART lives for the board's lifetime (not per
/// session), so this is a no-op.
unsafe extern "C" fn xrce_close(_user_data: *mut c_void) {}

/// XRCE `write`. Push `len` bytes from `buf` into the UART TX FIFO via
/// the existing `SerialPort::write` (`cmsdk-uart`'s blocking
/// FIFO-loop). Returns 0 on success (XRCE's by-`int32_t`-bytes ABI:
/// `< 0` on failure, `0` on success).
#[allow(static_mut_refs)]
unsafe extern "C" fn xrce_write(_user_data: *mut c_void, buf: *const u8, len: usize) -> i32 {
    if buf.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: caller guarantees `buf`/`len` form a valid readable region
    // for the duration of the call (XRCE custom-transport contract).
    let slice = unsafe { core::slice::from_raw_parts(buf, len) };
    // SAFETY: `UART_DEVICE` was initialised by the board crate during
    // `init_serial` before any XRCE session is opened; the
    // custom-transport contract serializes read/write so the &mut is
    // exclusive for the duration of the call.
    let written = unsafe { UART_DEVICE.assume_init_mut().write(slice) };
    if written == len { 0 } else { -1 }
}

/// XRCE `read`. Poll the CMSDK UART for up to `timeout_ms` waiting for
/// the first byte; once any data arrives, return what's available.
/// Returns the non-negative byte count on success, `0` on timeout (the
/// XRCE custom-transport contract treats `0` as a clean "no data" signal
/// the session loop retries on).
///
/// The wait is a busy poll against `clock_ms` — bare-metal mps2-an385 has
/// no scheduler to yield to; the session loop is single-threaded and the
/// CPU is dedicated. Without this active wait the handshake fails:
/// `CmsdkUart::read` is non-blocking, so a naive `return n as i32` returns
/// `0` immediately on every call, and XRCE's per-call timeout budget never
/// actually waits for `InitAck` to flow back through socat.
#[allow(static_mut_refs)]
unsafe extern "C" fn xrce_read(
    _user_data: *mut c_void,
    buf: *mut u8,
    len: usize,
    timeout_ms: u32,
) -> i32 {
    if buf.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: same reasoning as `xrce_write` for the buffer + UART access.
    let slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let deadline = nros_platform_mps2_an385::clock::clock_ms().saturating_add(timeout_ms as u64);
    loop {
        let n = unsafe { UART_DEVICE.assume_init_mut().read(slice) };
        if n > 0 {
            return n as i32;
        }
        if nros_platform_mps2_an385::clock::clock_ms() >= deadline {
            return 0;
        }
        core::hint::spin_loop();
    }
}

/// Phase 207.2 — return an `NrosRmwXrceTransportOps` bound to the board's
/// CMSDK UART0. Pass the result to
/// [`nros_rmw_xrce_cffi::set_custom_transport_ops`] (with `framing =
/// true` for the UART's byte-stream → XRCE HDLC framing) BEFORE the
/// first `Executor::open`.
pub fn xrce_transport_ops() -> nros_rmw_xrce_cffi::NrosRmwXrceTransportOps {
    nros_rmw_xrce_cffi::NrosRmwXrceTransportOps {
        user_data: core::ptr::null_mut(),
        open: Some(xrce_open),
        close: Some(xrce_close),
        write: Some(xrce_write),
        read: Some(xrce_read),
    }
}
