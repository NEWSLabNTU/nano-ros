//! Phase 241.D3-rev — single-runtime umbrella backend force-link + auto-register,
//! the nros-cpp twin of `nros-c::rmw_backend`.
//!
//! `nros-cpp` is the C++ umbrella's staticlib root. A `#[used]` anchor living in the
//! `nros-c` dependency is dead-code-eliminated before `libnros_cpp.a` is emitted, so
//! the root must do its own force-link: reference the selected backend's `register()`
//! (pulling its closure into the archive) and install an `.init_array` ctor so it
//! registers before `main`. cargo dedups the backend rlib with nros-c's copy.

#[used]
static FORCE_LINK: unsafe extern "C" fn() = auto_register;

// `pub` (W11, Option D) so the umbrella root can re-export it as
// `nros_cpp_auto_register_backend`: a per-entry `<entry>_runtime` staticlib bundles
// nros-cpp as a dep rlib, where this module's `.init_array` ctor below is DCE'd — the
// runtime root re-installs its own ctor pointing here. Within nros-cpp-as-root it stays
// the bundled backend's force-link + auto-register entry.
pub unsafe extern "C" fn auto_register() {
    #[cfg(feature = "rmw-zenoh-cffi")]
    {
        let _ = nros_rmw_zenoh::register();
    }
    #[cfg(feature = "rmw-xrce-cffi")]
    {
        let _ = nros_rmw_xrce_cffi::register();
    }
}

#[used]
#[cfg_attr(
    any(target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "none"),
    unsafe(link_section = ".init_array")
)]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__mod_init_func"))]
static AUTO_REGISTER_CTOR: unsafe extern "C" fn() = auto_register;
