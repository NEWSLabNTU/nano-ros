//! RMW backend self-registration (Phase 249 P4b.1 — `.init_array` ctor).
//!
//! Every `nros-rmw-<name>` crate (or C/C++ static lib) self-registers
//! its vtable with the cffi registry. The trigger differs by tier:
//!
//! - **Hosted (Rust + C/C++):** the backend emits a `#[used]` ctor
//!   function pointer into the platform loader's pre-`main` init
//!   section (`.init_array` on ELF). The loader fires every ctor before `main`, so each
//!   backend's [`crate::nros_rmw_cffi_register_named`] call has already
//!   run by the time the runtime opens a session. No runtime walk, no
//!   `linkme` distributed slice.
//!
//! - **Embedded (`target_os = "none"`):** the ctor expands to nothing.
//!   Bare-metal firmware has no loader that walks `.init_array` in the
//!   shape the registry needs, so the board / typed carrier calls
//!   `nros_rmw_<x>::register()` EXPLICITLY (phase-249 P1). The RTOS
//!   targets (NuttX / Zephyr / ESP-IDF / VxWorks) keep that explicit
//!   call too; the ctor there is harmless (register is idempotent).
//!
//! The native app keeps its `#[used] __FORCE_LINK_*` anchor (phase-244
//! D7 Shape B): the anchor defeats dead-code elimination so the backend
//! closure AND its ctor survive into the final binary — it is NOT a
//! registration call.
//!
//! This replaces the phase-128 `linkme` distributed-slice walker
//! (`RMW_INIT_ENTRIES` + `nros_rmw_cffi_walk_init_section`), removed in
//! phase-249 P4b.1 (RFC-0042 §D3.3).
//!
//! # C / C++ backends
//!
//! Static-lib backends emit their entry via the
//! [`NROS_RMW_REGISTER_BACKEND`] macro in `<nros/rmw_vtable.h>`; the
//! cmake `nano_ros_link_rmw` strong stub handles the C/C++-via-cmake
//! path (phase-249 P2b/P4a).
//!
//! [`NROS_RMW_REGISTER_BACKEND`]: ../../include/nros/rmw_vtable.h

/// Macro emitted by backend crates to self-register their vtable.
///
/// On hosted targets (`not(target_os = "none")`) it expands to a
/// `#[used]` ctor function pointer landed in the loader's pre-`main`
/// init section (`.init_array` on ELF). The loader invokes the ctor before `main`; the ctor body is
/// `$body`, which calls the backend's `register()`. On embedded
/// (`target_os = "none"`) it expands to nothing — the board calls
/// `register()` explicitly.
///
/// Usage (inside the backend crate):
///
/// ```ignore
/// nros_rmw_cffi::nros_rmw_register_backend! {
///     fn() { let _ = nros_rmw_zenoh_register(); }
/// }
/// ```
#[macro_export]
macro_rules! nros_rmw_register_backend {
    (fn() $body:block) => {
        // Hosted only. Bare-metal (`target_os = "none"`) has no loader
        // that fires `.init_array` for the registry, so the carrier
        // calls `register()` explicitly and the macro emits nothing.
        #[cfg(not(target_os = "none"))]
        const _: () = {
            unsafe extern "C" fn __nros_rmw_backend_auto_register() $body

            // The loader walks this section before transferring control
            // to `main`, invoking every fn pointer found there.
            // `#[used]` keeps the symbol against `--gc-sections`.
            // macOS/Apple dropped (phase-260) — ELF `.init_array` only.
            #[used]
            #[unsafe(link_section = ".init_array")]
            static __NROS_RMW_BACKEND_AUTO_REGISTER_CTOR: unsafe extern "C" fn() =
                __nros_rmw_backend_auto_register;
        };
    };
}
