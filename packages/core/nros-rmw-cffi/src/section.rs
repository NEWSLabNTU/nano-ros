//! Linker-section registry discovery (Phase 128.A).
//!
//! At static link time every `nros-rmw-<name>` crate or static lib
//! contributes exactly one [`RmwInitEntry`] function pointer to the
//! [`RMW_INIT_ENTRIES`] distributed slice. The runtime calls
//! `walk_init_section` / [`nros_rmw_cffi_walk_init_section`] on
//! first `Executor::open` / `nros::init` to invoke every entry; each
//! entry in turn calls [`crate::nros_rmw_cffi_register_named`] with
//! its canonical name and vtable pointer.
//!
//! This is *static* discovery — no `dlopen`, no plugin system. The
//! linker section is populated at link time and frozen for the
//! program's lifetime.
//!
//! The section anchoring (cross-platform `__start_/__stop_`
//! synthesis, `KEEP` semantics under `--gc-sections`, Mach-O
//! `__DATA,__nros_rmw_init` placement, COFF chunked sections) is
//! delegated to the [`linkme`] crate so the discipline works
//! identically on every target the crate supports: Linux / macOS /
//! Windows / FreeBSD / illumos / `target_os = "none"` (bare-metal
//! ELF).
//!
//! # Unsupported targets (RTOS / ESP-IDF)
//!
//! Targets `linkme` does NOT recognise (`target_os = "nuttx"`,
//! `"zephyr"`, `"espidf"`, `"vxworks"`, …) fall back to a no-op
//! [`RMW_INIT_ENTRIES`] stub and a walker that always returns 0.
//! Backends on those targets still register via the explicit
//! `nros_rmw_<x>::register()` call from main (the rlib-pull anchor
//! pattern documented in phase 128.B.1's commit) — the section
//! walker just doesn't add anything on top.
//!
//! # C / C++ backends
//!
//! Static-lib backends emit their entry via the
//! [`NROS_RMW_REGISTER_BACKEND`] macro in `<nros/rmw_vtable.h>`. The
//! macro lands the function pointer in `linkm2_RMW_INIT_ENTRIES`
//! (the section name `linkme` uses for [`RMW_INIT_ENTRIES`]),
//! interoperating with the Rust-side entries through pure linker
//! discipline.
//!
//! [`NROS_RMW_REGISTER_BACKEND`]: ../../include/nros/rmw_vtable.h

use core::sync::atomic::Ordering;

use portable_atomic::AtomicBool;

/// Public type of every entry in [`RMW_INIT_ENTRIES`]. The function
/// takes no arguments, returns nothing, and is expected to call
/// [`crate::nros_rmw_cffi_register_named`] with its own canonical
/// name + vtable. A non-zero return from the registration call is
/// silently ignored here — the walker only guarantees that every
/// entry is invoked; per-backend failures surface later at
/// `Executor::open` when the registry lookup misses.
pub type RmwInitEntry = unsafe extern "C" fn();

// ---------------------------------------------------------------------------
// Target-os gating for `linkme` support. The crate hard-codes the
// allow-list; mirror it here so unsupported targets get a no-op stub
// instead of a build break.
// ---------------------------------------------------------------------------

// Phase 134.fix — gate the linkme DEFINITION on both target-OS
// support AND the new `linkme-register` feature. Staticlibs that
// pre-link next to another archive (the canonical case:
// `libnros_rmw_zenoh.a` linked alongside `libnros_c.a`) turn the
// feature off to suppress the second DUPCHECK static and rely on
// `.init_array` auto-register instead.
#[cfg(all(
    feature = "linkme-register",
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "fuchsia",
        target_os = "psp",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "windows",
        target_os = "illumos",
        // Phase 142 — `target_os = "none"` DROPPED. Bare-metal
        // Cortex-M3 (and likely other no-OS targets) hangs
        // on `RMW_INIT_ENTRIES.iter()` inside `Executor::open`
        // because `cortex_m_rt`'s link script doesn't provide
        // the `__start_/__stop_` section anchors in a shape that
        // lets linkme's slice iterator terminate. Bare-metal
        // firmware uses the explicit `nros_rmw_<x>::register()`
        // call from `main()` (Phase 104.A pattern), so falling
        // into the stub path (empty slice, walker returns 0) is
        // the correct behaviour.
    )
))]
mod linkme_backed {
    use super::RmwInitEntry;
    use linkme::distributed_slice;

    /// Distributed slice that every `nros-rmw-<name>` crate (or
    /// C/C++ static lib via the `NROS_RMW_REGISTER_BACKEND` macro)
    /// contributes one entry to.
    #[distributed_slice]
    pub static RMW_INIT_ENTRIES: [RmwInitEntry] = [..];
}

#[cfg(not(all(
    feature = "linkme-register",
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "fuchsia",
        target_os = "psp",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "windows",
        target_os = "illumos",
        // Phase 142 — `target_os = "none"` DROPPED. Bare-metal
        // Cortex-M3 (and likely other no-OS targets) hangs
        // on `RMW_INIT_ENTRIES.iter()` inside `Executor::open`
        // because `cortex_m_rt`'s link script doesn't provide
        // the `__start_/__stop_` section anchors in a shape that
        // lets linkme's slice iterator terminate. Bare-metal
        // firmware uses the explicit `nros_rmw_<x>::register()`
        // call from `main()` (Phase 104.A pattern), so falling
        // into the stub path (empty slice, walker returns 0) is
        // the correct behaviour.
    )
)))]
mod linkme_backed {
    use super::RmwInitEntry;

    /// Stub used when `linkme-register` is off OR the target OS
    /// is not in linkme's supported set (NuttX, Zephyr, ESP-IDF,
    /// …). Always empty; the walker returns 0; backends register
    /// via `.init_array` (C-style ctor) or the explicit
    /// `register()` call pattern.
    pub static RMW_INIT_ENTRIES: [RmwInitEntry; 0] = [];
}

pub use linkme_backed::RMW_INIT_ENTRIES;

/// Macro emitted by backend crates to contribute one entry to
/// [`RMW_INIT_ENTRIES`]. Expands to a `linkme::distributed_slice`
/// item on targets `linkme` supports; expands to nothing on
/// unsupported targets (NuttX, Zephyr, ESP-IDF, …) where the
/// section walker is a no-op and backends must rely on the
/// explicit `register()` call pattern.
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
        // Phase 134.fix — also gate on the `linkme-register` Cargo
        // feature of `nros-rmw-cffi`. When the consuming staticlib
        // builds with `default-features = false`, the macro expands
        // to nothing (no linkme entry). Auto-registration falls to
        // an explicit `.init_array` ctor in the staticlib's lib.rs.
        #[cfg(all(
            feature = "linkme-register",
            any(
                target_os = "linux",
                target_os = "android",
                target_os = "fuchsia",
                target_os = "psp",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "ios",
                target_os = "tvos",
                target_os = "watchos",
                target_os = "windows",
                target_os = "illumos",
                // Phase 142 — `target_os = "none"` dropped here
                // too so the macro expansion in bare-metal
                // backend crates also collapses to a no-op,
                // matching the mod-gating above.
            )
        ))]
        const _: () = {
            unsafe extern "C" fn __nros_rmw_backend_section_entry() $body
            #[$crate::linkme::distributed_slice($crate::RMW_INIT_ENTRIES)]
            static __NROS_RMW_BACKEND_SECTION_ENTRY: $crate::RmwInitEntry =
                __nros_rmw_backend_section_entry;
        };
    };
}

/// Idempotency guard. Set after the first successful walk. Subsequent
/// `Executor::open` calls skip re-walking.
static WALKED: AtomicBool = AtomicBool::new(false);

/// Walk every entry in [`RMW_INIT_ENTRIES`]. Idempotent — subsequent
/// calls are no-ops. Safe to invoke from `Executor::open` /
/// `nros::init` on every entry into the runtime.
///
/// Returns the number of entries actually invoked on this call. A
/// return of 0 from the first call indicates no `nros-rmw-*` backend
/// was linked into this binary; the caller (typically
/// `Executor::open`'s resolution policy) should surface
/// [`crate::NROS_RMW_RET_NO_BACKEND`].
///
/// # Safety
///
/// Every entry in `RMW_INIT_ENTRIES` is contributed by an in-tree
/// `nros-rmw-<name>` crate (or static lib) via the
/// `nros_rmw_register_backend!` macro / `NROS_RMW_REGISTER_BACKEND`
/// C macro; both expand to a no-arg `extern "C" fn` whose only side
/// effect is calling [`crate::nros_rmw_cffi_register_named`]. Calling
/// the walker is therefore safe provided no third-party code has
/// injected a non-conforming entry into the same linker section.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_rmw_cffi_walk_init_section() -> usize {
    if WALKED.swap(true, Ordering::AcqRel) {
        return 0;
    }
    let mut invoked = 0usize;
    for entry in RMW_INIT_ENTRIES.iter() {
        // SAFETY: every backend's ctor calls
        // `nros_rmw_cffi_register_named`, which is documented as
        // safe to call before any session creation. The walker fires
        // before `Executor::open` finishes; nothing else races.
        unsafe { entry() };
        invoked += 1;
    }
    invoked
}

/// Test-only escape hatch — re-arms the walker so a second
/// `walk_init_section()` call re-invokes every entry. Hidden behind
/// `cfg(test)` because production callers must treat the walker as
/// fire-once.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn reset_walked_for_test() {
    WALKED.store(false, Ordering::Release);
}
