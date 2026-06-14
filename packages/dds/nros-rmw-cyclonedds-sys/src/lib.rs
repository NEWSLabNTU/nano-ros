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
//! C / C++ sources itself. The Cyclone DDS C++ library, the `cyclonedds-cxx`
//! layer, and the project's own register glue make the cmake build the
//! canonical entry point. Both
//! `packages/dds/nros-rmw-cyclonedds/CMakeLists.txt` (standalone POSIX path) and
//! `zephyr/CMakeLists.txt :: CONFIG_NROS_RMW_CYCLONEDDS` (Zephyr path) already
//! compile it. This crate just declares the Rust-facing symbol so consumers
//! (e.g. `nros-cpp`, future collapsed Rust examples) can link against the
//! resulting archive without their own bindgen pass.
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

// Phase 212.K.2 — keep the `cyclonedds-sys` rlib in the link graph so
// its build script's `cargo:rustc-link-lib=static=ddsc` (plus the
// dylib-link-libs for pthread/dl/rt) reach the final binary's link
// command. Cargo drops unreferenced rlibs from the link line; an
// `extern crate _` is the cheapest way to pin it in.
#[cfg(feature = "vendored")]
extern crate cyclonedds_sys as _;

// Phase 249 — re-anchor the posix C platform port. Mirrors
// `nros-rmw-zenoh::__FORCE_LINK_PLATFORM_CFFI`: `nros_node` references
// `nros_platform_wake_*`, provided by `libnros_platform_posix.a` (built by
// `nros-platform-cffi[posix-c-port]`). That archive is pulled by
// `nros_platform::__FORCE_LINK_CFFI`, but the `#[used]` static lives in the
// `nros-platform` rlib and is DCE'd from a binary root unless re-anchored.
// Cyclone's register path (unlike zenoh's) has no `nros-platform` dep, so
// native cyclone binaries lost the wake symbols (issue 0063). This `#[used]`
// re-anchor, gated on the native-only `platform-posix` feature, restores it.
// Embedded consumers keep the feature OFF and source the wake symbols from
// their own platform port.
#[cfg(feature = "platform-posix")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_PLATFORM_CFFI: extern "C" fn() = nros_platform::__FORCE_LINK_CFFI;

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
    // Phase 248 (C2) — install the per-type descriptor registrar into the
    // generic `nros_rmw` seam so the platform/RMW-agnostic core reaches
    // Cyclone's runtime descriptor builder without a named dep on the
    // Cyclone Rust shim.
    nros_rmw_cyclonedds::install_descriptor_registrar();
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
        // Phase 248 (C2) — wire the descriptor registrar at backend init.
        nros_rmw_cyclonedds::install_descriptor_registrar();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Phase 214 followup — when `vendored` is active (the default
    // post-214.S.1), the build script compiles + links the real
    // `libnros_rmw_cyclonedds.a` which provides
    // `nros_rmw_cyclonedds_register`. The legacy `cfg(test)` Rust
    // stub clashed with it at link time (rust-lld duplicate-symbol).
    // Only emit the stub when `vendored` is OFF — that's the only
    // configuration where the C++ archive is absent and the link
    // would otherwise fail with `undefined reference`.
    #[cfg(not(feature = "vendored"))]
    #[unsafe(no_mangle)]
    extern "C" fn nros_rmw_cyclonedds_register() -> c_int {
        0
    }

    #[test]
    fn register_succeeds_with_stub() {
        // Under vendored (default), this exercises the real C++ register
        // path against a freshly-built `libnros_rmw_cyclonedds.a`.
        // Under non-vendored, exercises the stub above.
        assert!(register().is_ok());
    }
}
