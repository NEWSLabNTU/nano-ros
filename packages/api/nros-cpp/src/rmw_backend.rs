//! Phase 241.D3-rev / 249 P3 — single-runtime umbrella backend force-link,
//! the nros-cpp twin of `nros-c::rmw_backend`.
//!
//! `nros-cpp` is the C++ umbrella's staticlib root. A `#[used]` anchor living in the
//! `nros-c` dependency is dead-code-eliminated before `libnros_cpp.a` is emitted, so
//! the root must do its own force-link: reference the selected backend's `register()`
//! to pull its closure (and its `#[no_mangle]` `nros_rmw_<x>_register` C export) into
//! the archive. cargo dedups the backend rlib with nros-c's copy.
//!
//! Phase 249 P3: the `.init_array` auto-register ctor is **retired** — registration is
//! the one universal explicit call (the generated `nros_app_register_backends()` strong
//! def, P2b, invoking `nros_rmw_<x>_register()`). `FORCE_LINK` + the `pub auto_register`
//! re-export stay (W11 Option D: the per-configure `nros_ws_runtime` staticlib anchors
//! `nros_cpp_auto_register_backend` to keep the backend closure linked for the call).

#[used]
static FORCE_LINK: unsafe extern "C" fn() = auto_register;

/// Force-link anchor for the selected cffi backend's closure (incl. its
/// `nros_rmw_<x>_register` C export).
///
/// `pub` (W11, Option D) so the umbrella root can re-export it as
/// `nros_cpp_auto_register_backend`: the per-configure `nros_ws_runtime` staticlib
/// `#[used]`-anchors that re-export to keep the backend closure past staticlib DCE, so
/// the generated `nros_app_register_backends()` C stub can resolve `nros_rmw_<x>_register`.
///
/// # Safety
/// Takes no arguments and dereferences no pointers — the `unsafe` marker only reflects its
/// `extern "C"` ABI. Safe to call any number of times; the backend `register()` it forwards
/// to is idempotent. It is the body the `nros_app_register_backends` path ultimately reaches.
pub unsafe extern "C" fn auto_register() {
    #[cfg(feature = "rmw-zenoh-cffi")]
    {
        let _ = nros_rmw_zenoh::register();
    }
    #[cfg(feature = "rmw-xrce-cffi")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
    // phase-313 — the recording backend needs the same force-link as the real
    // ones. Being an optional DEP is not enough: nothing referenced it, so
    // rustc's staticlib DCE dropped its `#[no_mangle]` export and the probe
    // binary linked only zenoh (`nm` showed no
    // `nros_rmw_metadata_register`). The `.init_array` ctor cannot fire for
    // code that is not in the image. Same class as the FORCE_LINK note in
    // CLAUDE.md for pure-Rust images.
    #[cfg(feature = "metadata-mode")]
    {
        let _ = nros_rmw_metadata::nros_rmw_metadata_register();
    }
}
