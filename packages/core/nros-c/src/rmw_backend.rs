//! Phase 241.D3-rev / 249 P3 — single-runtime umbrella backend force-link.
//!
//! `nros-c` is the staticlib root; an unreferenced backend rlib dependency would be
//! dead-code-eliminated out of `libnros_c.a` entirely. This module references the
//! selected backend's `register()` via a `#[used]` anchor so its closure (the cffi
//! vtable install + the transport, e.g. zenoh-pico) — and the backend's
//! `#[no_mangle]` C export `nros_rmw_<x>_register` — are pulled into the archive.
//!
//! Phase 249 P3: the `.init_array` auto-register ctor is **retired**. Registration
//! is now the one universal explicit call — the generated strong
//! `nros_app_register_backends()` (P2b, every C/C++ app via the shared link path)
//! invokes `nros_rmw_<x>_register()` directly. The ctor was the fragile path
//! (`.init_array` is not walked on bare-metal/RTOS — the #48 hazard). `FORCE_LINK`
//! stays: it keeps that C export present for the explicit call to resolve.

/// Force-link anchor: keeps the linked backend's closure (incl. its `#[no_mangle]`
/// `nros_rmw_<x>_register` C export) in `libnros_c.a` past DCE, so the generated
/// `nros_app_register_backends()` strong def can call it on every target.
#[used]
static FORCE_LINK: unsafe extern "C" fn() = auto_register;

unsafe extern "C" fn auto_register() {
    #[cfg(feature = "rmw-zenoh")]
    {
        let _ = nros_rmw_zenoh::register();
    }
    #[cfg(feature = "rmw-xrce")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
}
