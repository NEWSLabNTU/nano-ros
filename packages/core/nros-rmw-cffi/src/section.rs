//! Linker-section registry discovery (Phase 128.A).
//!
//! At static link time every `nros-rmw-<name>` crate or static lib
//! emits exactly one entry into the `.nros_rmw_init` section
//! (`__DATA,__nros_rmw_init` on Mach-O). Each entry is a function
//! pointer of the type [`RmwInitEntry`]. The runtime calls
//! [`walk_init_section`] on first `Executor::open` / `nros::init` to
//! invoke every entry; each entry in turn calls
//! [`crate::nros_rmw_cffi_register_named`] with its canonical name
//! and vtable pointer.
//!
//! This is *static* discovery — no `dlopen`, no plugin system. The
//! section is populated at link time and frozen for the program's
//! lifetime.
//!
//! # Embedded targets
//!
//! Cortex-M / RISC-V / Xtensa linker scripts that use
//! `--gc-sections` must keep the section live with
//! `KEEP(.nros_rmw_init)`. The snippet
//! `cmake/nros-rmw-section.ld` ships ready-made; board crates
//! `INCLUDE` it from their `memory.x`. See
//! `book/src/reference/rmw-backends.md`.

use core::sync::atomic::Ordering;

use portable_atomic::AtomicBool;

/// Public type of every entry in the `.nros_rmw_init` section. The
/// function takes no arguments, returns nothing, and is expected to
/// call [`crate::nros_rmw_cffi_register_named`] with its own
/// canonical name + vtable. A non-zero return from the registration
/// call is silently ignored here — the walker only guarantees that
/// every entry is invoked; per-backend failures surface later at
/// `Executor::open` when the registry lookup misses.
pub type RmwInitEntry = unsafe extern "C" fn();

/// Idempotency guard. Set after the first successful walk. Subsequent
/// `Executor::open` calls skip re-walking.
static WALKED: AtomicBool = AtomicBool::new(false);

/// Sentinel anchor so the `.nros_rmw_init` section always has at least
/// one input. Without this, LLD's start/stop encapsulation strips the
/// section when no backend is linked, and the `__start_*` / `__stop_*`
/// symbols become undefined references. The walker treats the sentinel
/// as a no-op (skips entries whose target == `sentinel_entry`).
unsafe extern "C" fn sentinel_entry() {}

#[used]
#[cfg_attr(
    target_os = "macos",
    unsafe(link_section = "__DATA,__nros_rmw_init"),
)]
#[cfg_attr(
    not(target_os = "macos"),
    unsafe(link_section = ".nros_rmw_init"),
)]
static SECTION_ANCHOR: RmwInitEntry = sentinel_entry;

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "netbsd"))]
unsafe extern "C" {
    #[link_name = "__start_nros_rmw_init"]
    static __start_nros_rmw_init: RmwInitEntry;
    #[link_name = "__stop_nros_rmw_init"]
    static __stop_nros_rmw_init: RmwInitEntry;
}

#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "netbsd"))]
fn section_bounds() -> (*const RmwInitEntry, *const RmwInitEntry) {
    // SAFETY: `__start_*` and `__stop_*` are linker-synthesised
    // address symbols for the `.nros_rmw_init` section. Reading their
    // address (not their value) is always safe.
    let start = unsafe { &__start_nros_rmw_init as *const RmwInitEntry };
    let stop = unsafe { &__stop_nros_rmw_init as *const RmwInitEntry };
    (start, stop)
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    // Mach-O does not synthesise `__start_/__stop_` automatically; the
    // backend-side `#[link_section = "__DATA,__nros_rmw_init"]` entries
    // are anchored via `getsectiondata` at runtime instead. We approximate
    // the ELF interface with a pair of symbols emitted by a small
    // companion build-script-compiled object (`build_section_macos.c`)
    // that links `__section_start_nros_rmw_init` / `__section_stop_*`
    // via the `__attribute__((section))` self-anchor trick.
    #[link_name = "__nros_rmw_init_section_start"]
    static __start_nros_rmw_init: RmwInitEntry;
    #[link_name = "__nros_rmw_init_section_stop"]
    static __stop_nros_rmw_init: RmwInitEntry;
}

#[cfg(target_os = "macos")]
fn section_bounds() -> (*const RmwInitEntry, *const RmwInitEntry) {
    let start = unsafe { &__start_nros_rmw_init as *const RmwInitEntry };
    let stop = unsafe { &__stop_nros_rmw_init as *const RmwInitEntry };
    (start, stop)
}

// Bare-metal / RTOS targets: the linker-script fragment shipped in
// `cmake/nros-rmw-section.ld` defines both anchor symbols. If the
// fragment is missing, the program will fail to link with an
// "undefined reference to `__nros_rmw_init_start`" error, which
// surfaces the misconfiguration at build time rather than as a silent
// empty registry at runtime.
#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "macos",
)))]
unsafe extern "C" {
    #[link_name = "__nros_rmw_init_start"]
    static __start_nros_rmw_init: RmwInitEntry;
    #[link_name = "__nros_rmw_init_stop"]
    static __stop_nros_rmw_init: RmwInitEntry;
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "macos",
)))]
fn section_bounds() -> (*const RmwInitEntry, *const RmwInitEntry) {
    let start = unsafe { &__start_nros_rmw_init as *const RmwInitEntry };
    let stop = unsafe { &__stop_nros_rmw_init as *const RmwInitEntry };
    (start, stop)
}

/// Walk every entry in `.nros_rmw_init`. Idempotent — subsequent
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
    // Force the rlib compilation unit that owns `SECTION_ANCHOR` to
    // be pulled into every downstream binary. Without this, lld omits
    // the unit (no other symbol references it), the `.nros_rmw_init`
    // section is empty in the output, and the `__start_/__stop_*`
    // encapsulation symbols are undefined.
    core::hint::black_box(&SECTION_ANCHOR);
    if WALKED.swap(true, Ordering::AcqRel) {
        return 0;
    }
    let (start, stop) = section_bounds();
    // `stop - start` is the count of `RmwInitEntry` slots placed in
    // the section. Linker semantics guarantee `stop >= start`; the
    // difference is in units of `RmwInitEntry`, not bytes.
    //
    // SAFETY: pointers refer to a contiguous, statically allocated
    // region of function pointers. Both anchors come from the linker;
    // their addresses are program-lifetime stable.
    let count = unsafe { stop.offset_from(start) } as usize;
    let mut i = 0usize;
    let mut invoked = 0usize;
    while i < count {
        // SAFETY: i < count = stop - start, so the offset is in
        // bounds. The read produces a valid function pointer placed
        // there at link time by a backend's `RmwInitEntry` static.
        let entry = unsafe { *start.add(i) };
        i += 1;
        // Skip the in-crate sentinel anchor (see `SECTION_ANCHOR`
        // above) — it exists only to keep the start/stop symbols
        // resolved under LLD start-stop-gc, not as a real backend.
        if (entry as *const ()) == (sentinel_entry as *const ()) {
            continue;
        }
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
