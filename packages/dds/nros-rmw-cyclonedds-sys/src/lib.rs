//! Phase 169.5 — Rust shim that exposes the Cyclone DDS C++ backend's
//! C-linkage register entry to Rust callers.
//!
//! Mirrors `nros-rmw-xrce-cffi`. Cyclone DDS's
//! `nros_rmw_cyclonedds_register()` (declared in
//! `packages/dds/nros-rmw-cyclonedds/include/nros_rmw_cyclonedds.h`)
//! is `extern "C"` because the underlying C++ implementation
//! deliberately exports a C ABI for the same kind of dispatch the
//! XRCE / Zenoh paths already use.
//!
//! Unlike `nros-rmw-xrce-cffi`, this crate does **not** compile any
//! C / C++ sources itself. The Cyclone DDS C++ library and the
//! `nros-rmw-cyclonedds` shim are heavy enough (cyclonedds submodule
//! + cyclonedds-cxx + the project's own register glue) that their
//! cmake build is the canonical entry point — both
//! `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt` (standalone
//! POSIX path) and `zephyr/CMakeLists.txt :: CONFIG_NROS_RMW_CYCLONEDDS`
//! (Zephyr path) already compile it. This crate just declares the
//! Rust-facing symbol so consumers (e.g. `nros-cpp`, future
//! collapsed Rust examples) can link against the resulting archive
//! without their own bindgen pass.
//!
//! # Use
//!
//! ```no_run
//! // Once at startup, before opening any Executor:
//! nros_rmw_cyclonedds_sys::register().expect("cyclonedds register");
//!
//! // Then create the session through nros's normal cffi-rmw path.
//! ```
//!
//! Idempotent: calling twice re-registers the same vtable, which
//! the runtime treats as a no-op.

#![cfg_attr(not(feature = "std"), no_std)]

use core::ffi::c_int;

unsafe extern "C" {
    /// C entry point declared in
    /// `packages/dds/nros-rmw-cyclonedds/include/nros_rmw_cyclonedds.h`
    /// — implemented in `packages/dds/nros-rmw-cyclonedds/src/`
    /// (C++ source with `extern "C"` linkage). Returns
    /// `NROS_RMW_RET_OK` (0) on success.
    fn nros_rmw_cyclonedds_register() -> c_int;
}

/// Failure mode when the runtime rejects the vtable. Mirrors
/// `NROS_RMW_RET_*` in `packages/core/nros-rmw-cffi/include/nros/rmw_ret.h`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RegisterError(pub c_int);

/// Register the Cyclone DDS C++ backend with the nros runtime.
/// Returns `Err(RegisterError(rc))` if the runtime rejected the
/// vtable (e.g. unknown ABI version on a future minor bump).
pub fn register() -> Result<(), RegisterError> {
    // SAFETY: `nros_rmw_cyclonedds_register` is a no-arg C entry
    // point declared in the project header above; it hands a
    // static, immutable vtable to `nros_rmw_cffi_register` inside
    // its C++ TU. No invariants required from the caller beyond
    // "call before opening any session" — that is the runtime
    // contract documented in `book/src/internals/rmw-backends.md`.
    let rc = unsafe { nros_rmw_cyclonedds_register() };
    if rc == 0 {
        Ok(())
    } else {
        Err(RegisterError(rc))
    }
}

// Phase 128.B.3 / 128.H.2 — `RMW_INIT_ENTRIES` self-registration
// via `nros_rmw_register_backend!` (RTOS-target-safe; gated off
// during `cargo test` to avoid double-registration in unit tests).
#[cfg(not(test))]
nros_rmw_cffi::nros_rmw_register_backend! {
    fn() {
        let _ = unsafe { nros_rmw_cyclonedds_register() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Stub out the C symbol so `cargo test` links cleanly on hosted
    // targets without dragging in the full Cyclone DDS C++ closure.
    #[unsafe(no_mangle)]
    extern "C" fn nros_rmw_cyclonedds_register() -> c_int {
        0
    }

    #[test]
    fn register_succeeds_with_stub() {
        assert!(register().is_ok());
    }
}
