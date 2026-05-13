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

#![no_std]

use core::ffi::c_int;

unsafe extern "C" {
    /// C entry point exported by `packages/xrce/nros-rmw-xrce/src/vtable.c`.
    /// Calls `nros_rmw_cffi_register_named("xrce", &kVtable)` internally.
    fn nros_rmw_xrce_register() -> c_int;
}

/// Failure mode when the runtime rejects the vtable.
///
/// Mirrors `NROS_RMW_RET_*` constants from
/// `packages/core/nros-rmw-cffi/include/nros/rmw_ret.h`. v1 is opaque
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

// Phase 104.A — POSIX auto-registration. See
// `nros-rmw-zenoh/src/lib.rs::cffi_register` for the rationale.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[used]
#[unsafe(link_section = ".init_array")]
static AUTO_REGISTER_CTOR: extern "C" fn() = auto_register_ctor;

#[cfg(target_os = "macos")]
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static AUTO_REGISTER_CTOR: extern "C" fn() = auto_register_ctor;

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "macos"))]
extern "C" fn auto_register_ctor() {
    let _ = unsafe { nros_rmw_xrce_register() };
}
