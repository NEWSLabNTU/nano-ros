//! Phase 115.K.2.5.1.0 — Rust shim that links the C XRCE backend
//! ([`nros-rmw-xrce`]) and exposes its register entry point.
//!
//! Mirrors the role the cyclonedds backend would play if it had Rust
//! users — it doesn't, so this is the project's first cffi-shim crate.
//! The shape is the canonical template for future backends that go
//! Rust API → cffi → native-language vtable consumer.
//!
//! # Use
//!
//! ```no_run
//! // Once at startup, before opening any [`Executor`]:
//! nros_rmw_xrce_cffi::register().expect("xrce-c register");
//!
//! // Then create the session through nros's normal cffi-rmw path.
//! ```
//!
//! `register()` is idempotent — calling it twice re-registers the
//! same vtable, which the runtime treats as a no-op.

#![cfg_attr(not(feature = "std"), no_std)]

use core::ffi::{c_int, c_void};

unsafe extern "C" {
    /// C entry point exported by `packages/xrce/nros-rmw-xrce/src/vtable.c`.
    /// Calls `nros_rmw_cffi_register_named("xrce", &kVtable)` internally.
    fn nros_rmw_xrce_register() -> c_int;

    /// Phase 207.1 — C entry point exported by
    /// `packages/xrce/nros-rmw-xrce/src/transport_custom.c`. Installs a
    /// custom-transport vtable for the XRCE backend's `custom://` /
    /// `serial/...` locator path. After this call returns OK, the backend
    /// routes session I/O through the supplied callbacks instead of UDP /
    /// POSIX-serial.
    ///
    /// `framing` is `1` for byte-stream transports (UART / USB-CDC) that
    /// need XRCE's HDLC framing, `0` for packet-oriented transports.
    fn nros_rmw_xrce_set_custom_transport_ops(
        ops: *const NrosRmwXrceTransportOps,
        framing: c_int,
    ) -> c_int;
}

/// Phase 207.1 / 115.K.2.4 — runtime transport vtable for the XRCE
/// custom-transport bridge. Layout-identical to
/// `nros_rmw_xrce_transport_ops_t` in
/// `packages/xrce/nros-rmw-xrce/include/nros_rmw_xrce.h`. Pass to
/// [`set_custom_transport_ops`] (or, in `extern "C"` consumers, to
/// `nros_rmw_xrce_set_custom_transport_ops` directly).
///
/// Field semantics (mirrors the C doc):
/// - `user_data` — opaque caller context, threaded back as the first arg
///   into every callback; must outlive the transport's active period.
/// - `open` / `close` — open / tear down the underlying medium. `open`
///   returns `0` on success, negative `nros_rmw_ret_t` on failure.
/// - `write` — send `len` bytes; `0` ok, negative `nros_rmw_ret_t` on fail.
/// - `read` — receive up to `len` bytes within `timeout_ms`; non-negative
///   byte count on success, negative `nros_rmw_ret_t` on error / timeout.
///
/// Threading: `read` / `write` are never invoked concurrently; callbacks
/// must not run from interrupt context.
#[repr(C)]
#[derive(Debug)]
pub struct NrosRmwXrceTransportOps {
    pub user_data: *mut c_void,
    pub open: Option<unsafe extern "C" fn(user_data: *mut c_void, params: *const c_void) -> i32>,
    pub close: Option<unsafe extern "C" fn(user_data: *mut c_void)>,
    pub write:
        Option<unsafe extern "C" fn(user_data: *mut c_void, buf: *const u8, len: usize) -> i32>,
    pub read: Option<
        unsafe extern "C" fn(
            user_data: *mut c_void,
            buf: *mut u8,
            len: usize,
            timeout_ms: u32,
        ) -> i32,
    >,
}

/// Failure mode when the runtime rejects the vtable.
///
/// Mirrors `NROS_RMW_RET_*` constants from
/// `packages/core/nros-rmw-abi/include/nros/rmw_ret.h`. v1 is opaque
/// — callers should treat any non-zero return as a hard failure and
/// abort startup.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub c_int);

/// Register the micro-XRCE-DDS-Client C backend with the nros
/// runtime. Returns `Err(RegisterError(rc))` if the runtime rejected
/// the vtable (e.g. unknown ABI version on a future minor bump).
pub fn register() -> Result<(), RegisterError> {
    // SAFETY: `nros_rmw_xrce_register` is a no-arg C entry point that
    // returns an `int`. It internally hands a static, immutable
    // vtable to `nros_rmw_cffi_register`. No invariants required from
    // the caller beyond "call before opening any session" — that is
    // the runtime contract documented in
    // `book/src/internals/rmw-backends.md` (Phase 115.K.1).
    let rc = unsafe { nros_rmw_xrce_register() };
    if rc == 0 {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}

// Phase 128.B.3 / 128.H.2 — `RMW_INIT_ENTRIES` self-registration
// via `nros_rmw_register_backend!` (RTOS-target-safe).
#[cfg(not(test))]
nros_rmw_cffi::nros_rmw_register_backend! {
    fn() {
        let _ = unsafe { nros_rmw_xrce_register() };
    }
}

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn nros_rmw_cffi_register_named(
    _name: *const core::ffi::c_char,
    _vtable: *const core::ffi::c_void,
) -> c_int {
    0
}

/// Phase 207.1 — install a custom transport vtable for the XRCE backend.
///
/// Must be called BEFORE [`register()`] / `Executor::open`. The XRCE
/// backend's `custom://` (or `serial/...`) locator then drives session I/O
/// through `ops`'s callbacks instead of the POSIX UDP / SERIAL profiles
/// (which aren't compiled on `target_os = "none"` anyway — see
/// `build.rs`'s `is_posix` gating). This is the only XRCE transport
/// surface available on bare-metal targets.
///
/// `framing = true` selects HDLC framing (UART / USB-CDC); `false` for
/// packet-oriented links.
///
/// The struct is copied into backend-local storage on the C side; the
/// caller may free or mutate `ops` after this call returns. The fn-pointer
/// targets, however, must remain valid until the session closes.
///
/// Idempotent — calling twice replaces the previous registration.
///
/// # Safety
///
/// The callbacks in `ops` must satisfy the XRCE custom-transport contract
/// (see `nros_rmw_xrce.h`): no concurrent `read`/`write` from different
/// threads, no invocation from interrupt context, `user_data` lives until
/// `close` returns.
pub unsafe fn set_custom_transport_ops(
    ops: &NrosRmwXrceTransportOps,
    framing: bool,
) -> Result<(), RegisterError> {
    // SAFETY: `ops` is a valid reference for the duration of the call; the
    // C side copies the struct fields into file-scope storage before
    // returning, so the borrow does not need to outlive the call. The
    // caller upholds the fn-pointer-lifetime + threading contract above.
    let rc = unsafe { nros_rmw_xrce_set_custom_transport_ops(ops as *const _, framing as c_int) };
    if rc == 0 {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}
