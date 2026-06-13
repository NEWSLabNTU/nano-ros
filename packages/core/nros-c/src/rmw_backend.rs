//! Phase 241.D3-rev — single-runtime umbrella backend force-link + auto-register.
//!
//! `nros-c` is the staticlib root; an unreferenced backend rlib dependency would be
//! dead-code-eliminated out of `libnros_c.a` entirely. This module references the
//! selected backend's `register()` so its closure (the cffi vtable install + the
//! transport, e.g. zenoh-pico) is pulled into the archive, and installs an
//! `.init_array` ctor so the backend registers before `main` — the same idiom the
//! retired `nros-rmw-{zenoh,xrce}-cffi-staticlib` wrappers used, now folded in.

/// Register the linked backend. Referenced by the `.init_array` ctor (hosted) and
/// kept via `#[used]` so the backend closure survives DCE on every target — even
/// bare-metal ELF where the board startup, not the loader, walks `.init_array`.
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

// Hosted ELF / Mach-O: the loader walks the init section before `main`, so the
// backend self-registers with no board cooperation. Bare-metal (`target_os =
// "none"`) relies on the board startup walking `.init_array`; `FORCE_LINK` above
// still guarantees the symbols are present for it (or for a manual call).
#[used]
#[cfg_attr(
    any(target_os = "linux", target_os = "android", target_os = "freebsd", target_os = "none"),
    unsafe(link_section = ".init_array")
)]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__mod_init_func"))]
static AUTO_REGISTER_CTOR: unsafe extern "C" fn() = auto_register;
