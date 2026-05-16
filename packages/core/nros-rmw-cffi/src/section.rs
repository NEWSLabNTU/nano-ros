//! Linker-section registry discovery (Phase 128.A).
//!
//! At static link time every `nros-rmw-<name>` crate or static lib
//! contributes exactly one [`RmwInitEntry`] function pointer to the
//! [`RMW_INIT_ENTRIES`] distributed slice. The runtime calls
//! [`walk_init_section`] / [`nros_rmw_cffi_walk_init_section`] on
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
//! identically on POSIX ELF, macOS, Windows, and bare-metal /
//! `target_os = "none"` targets without per-target linker-script
//! fragments.
//!
//! # C / C++ backends
//!
//! Static-lib backends emit their entry via the
//! [`NROS_RMW_REGISTER_BACKEND`] macro in `<nros/rmw_vtable.h>`. The
//! macro lands the function pointer in `linkme_NROS_RMW_REGISTER`
//! (the section name `linkme` uses for [`RMW_INIT_ENTRIES`]),
//! interoperating with the Rust-side entries through pure linker
//! discipline.
//!
//! [`NROS_RMW_REGISTER_BACKEND`]: ../../include/nros/rmw_vtable.h

use core::sync::atomic::Ordering;

use linkme::distributed_slice;
use portable_atomic::AtomicBool;

/// Public type of every entry in [`RMW_INIT_ENTRIES`]. The function
/// takes no arguments, returns nothing, and is expected to call
/// [`crate::nros_rmw_cffi_register_named`] with its own canonical
/// name + vtable. A non-zero return from the registration call is
/// silently ignored here — the walker only guarantees that every
/// entry is invoked; per-backend failures surface later at
/// `Executor::open` when the registry lookup misses.
pub type RmwInitEntry = unsafe extern "C" fn();

/// Distributed slice that every `nros-rmw-<name>` crate (or C/C++
/// static lib via the `NROS_RMW_REGISTER_BACKEND` macro) contributes
/// one entry to. The runtime walks the slice on first
/// `Executor::open`. Empty slice = no backend linked → resolution
/// returns [`crate::NROS_RMW_RET_NO_BACKEND`].
#[distributed_slice]
pub static RMW_INIT_ENTRIES: [RmwInitEntry] = [..];

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
