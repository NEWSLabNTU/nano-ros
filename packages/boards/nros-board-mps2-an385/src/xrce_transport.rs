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
/// The wait sleeps on UART0's RX interrupt and a SysTick one-shot
/// (phase-521 W5, `tickless`); bare-metal mps2-an385 has no scheduler to
/// yield to, so before that it was a busy poll against `clock_ms`. Without this active wait the handshake fails:
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
        let now = nros_platform_mps2_an385::clock::clock_ms();
        if now >= deadline {
            return 0;
        }
        // phase-521 W5 — sleep until the next byte or the deadline on UART0's
        // RX interrupt and a one-shot, instead of spinning on the UART.
        #[cfg(not(feature = "rtic"))]
        unsafe {
            crate::tickless::serial_wait_idle((deadline - now) * 1000)
        };
        #[cfg(feature = "rtic")]
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

/// phase-521 W5 (RFC-0113 D4) — the executor's wake callback while a session
/// rides this transport, set through [`xrce_set_rx_notify`].
static RX_NOTIFY_CB: core::sync::atomic::AtomicPtr<()> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static RX_NOTIFY_CTX: core::sync::atomic::AtomicPtr<core::ffi::c_void> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// The transport's receive notification
/// ([`nros_rmw_xrce_cffi::set_custom_transport_rx_notify`]): the backend hands
/// it the executor's wake callback, and UART0's RX interrupt calls it
/// ([`notify_rx`]). The port's wake signal is an atomic store, safe there.
///
/// # Safety
/// The XRCE backend's contract: `ctx` stays valid until a call with
/// `cb == None` returns.
pub unsafe extern "C" fn xrce_set_rx_notify(
    _user_data: *mut core::ffi::c_void,
    cb: Option<unsafe extern "C" fn(ctx: *mut core::ffi::c_void)>,
    ctx: *mut core::ffi::c_void,
) -> i32 {
    use core::sync::atomic::Ordering;
    // Clear the callback first and set it last, so the ISR never pairs a new
    // callback with an old context.
    RX_NOTIFY_CB.store(core::ptr::null_mut(), Ordering::Release);
    RX_NOTIFY_CTX.store(ctx, Ordering::Release);
    if let Some(cb) = cb {
        RX_NOTIFY_CB.store(cb as *mut (), Ordering::Release);
    }
    0
}

/// Called by UART0's RX interrupt: tell the session's executor data arrived.
pub(crate) fn notify_rx() {
    use core::sync::atomic::Ordering;
    let cb = RX_NOTIFY_CB.load(Ordering::Acquire);
    if cb.is_null() {
        return;
    }
    let ctx = RX_NOTIFY_CTX.load(Ordering::Acquire);
    // SAFETY: stored from a `Some(cb)` by `xrce_set_rx_notify`; the backend
    // keeps `ctx` valid while it is installed.
    let cb: unsafe extern "C" fn(*mut core::ffi::c_void) = unsafe { core::mem::transmute(cb) };
    unsafe { cb(ctx) };
}
