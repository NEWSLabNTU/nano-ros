//! phase-313 — force-link for the RECORDING backend (`metadata-mode`).
//!
//! `nros-cpp` is the C++ umbrella's staticlib root, so an optional dependency
//! nothing references is dead-code-eliminated out of `libnros_cpp.a`: the
//! probe binary then linked no `nros_rmw_metadata_register` and the backend's
//! `.init_array` self-registration was never in the image. This anchor keeps it.
//!
//! The TRANSPORT backends used to be force-linked here too, one `cfg` arm per
//! backend. RFC-0071 D9 (phase-506 W4d) moved that to the image crate
//! `cmake/NanoRosImageCrate.cmake` generates, which depends on the backend the
//! image's descriptor names and anchors its `register()` itself — so this crate
//! names no backend (issue 1297). The recorder stays: it is not a backend an
//! image selects, it is what `metadata-mode` means.

#[used]
static FORCE_LINK: unsafe extern "C" fn() = auto_register;

/// Force-link anchor for the recording backend's closure.
///
/// `pub` so the umbrella root can list it in `FORCE_LINK_ANCHOR`, which a
/// downstream staticlib root (`nros_ws_runtime`, an image crate) re-pulls past
/// DCE.
///
/// # Safety
/// Takes no arguments and dereferences no pointers — the `unsafe` marker only
/// reflects its `extern "C"` ABI. Safe to call any number of times.
pub unsafe extern "C" fn auto_register() {
    let _ = nros_rmw_metadata::nros_rmw_metadata_register();
}
