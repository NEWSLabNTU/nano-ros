//! # nros-c
//!
//! C API for nros, providing an rclc-compatible interface for embedded systems.
//!
//! This crate exposes the nros functionality through a C-compatible FFI interface,
//! allowing C applications to use nros with familiar ROS 2 patterns.
//!
//! # Safety
//!
//! All unsafe functions in this crate follow C FFI conventions. Callers must:
//! - Ensure all pointers are valid and properly aligned
//! - Follow the initialization/finalization order documented for each type
//! - Not use objects after they have been finalized

#![no_std]
#![allow(non_camel_case_types)]
// FFI crate - many functions are unsafe extern "C" by necessity
#![allow(clippy::missing_safety_doc)]
// Dead code warnings for internal helpers that may be used later
#![allow(dead_code)]
// Edition 2024: This crate is a pure C FFI wrapper with 420+ unsafe operations in
// unsafe extern "C" functions. Adding explicit unsafe blocks would add significant
// verbosity without meaningful safety improvement, since all callers already need
// to provide the necessary safety guarantees.
#![allow(unsafe_op_in_unsafe_fn)]
// Executor spin loops depend on external state changes (e.g., from another thread calling stop)
#![allow(clippy::while_immutable_condition)]

// ── Crate-level imports ─────────────────────────────────────────────────

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "panic-halt")]
use panic_halt as _;

// Phase 241.D3-rev — single-runtime umbrella: force-link the selected RMW backend
// rlib into this staticlib and auto-register it before `main`. `nros-c` is the
// staticlib root, so an unreferenced backend rlib is DCE'd entirely; `rmw_backend`
// references the backend's `register()` (pulling its closure + the cffi vtable
// install) and installs an `.init_array` ctor. Folds in the retired
// `nros-rmw-{zenoh,xrce}-cffi-staticlib` wrappers.
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce"))]
mod rmw_backend;

#[cfg(any(feature = "alloc", feature = "std"))]
extern crate alloc;

// Opt-in RTOS heap-usage tracking (issue #6). A single shared `HeapStats`
// counter instruments whichever RTOS global allocator is active (exactly one
// platform feature is on at a time). `STATS` sees the Rust global allocator's
// footprint only — zenoh-pico's direct C-side z_malloc/pvPortMalloc traffic is
// not counted, so it under-reports true heap pressure.
//
// Phase 230 1b.3 / RFC-0034 D7 — the platform ABI exposes the TRUE *unified*
// heap figures (`nros_platform_heap_used_bytes` / `_total_bytes`), where the
// platform owns one kernel heap shared by the C side and the Rust
// `#[global_allocator]`. Design: keep `nros_heap_used_bytes()` /
// `nros_heap_peak_bytes()` as the Rust-footprint view (unchanged semantics, so
// callers tracking only the Rust allocator keep their meaning) and add
// `nros_heap_platform_used_bytes()` + `nros_heap_total_bytes()` that forward to
// the platform query for the unified figure. Both return `0` on ports that
// don't instrument their heap.
//
// phase-341 W8.c — the counter itself moved to `nros-platform` with the
// allocator it instruments (there is only one allocator now). What stays here
// is the C surface: these `#[no_mangle]` names are part of the C/C++ API and
// must keep coming from this crate.
#[cfg(feature = "alloc-stats")]
mod heap_stats {
    // Canonical platform heap query (RFC-0034 D7). Resolved at the final
    // C-binary link step from the linked `nros-platform-<rtos>` cffi shim.
    unsafe extern "C" {
        fn nros_platform_heap_used_bytes() -> usize;
        fn nros_platform_heap_total_bytes() -> usize;
    }

    /// Bytes currently outstanding through the Rust global allocator.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_used_bytes() -> usize {
        nros_platform::heap_stats::used()
    }

    /// Peak outstanding bytes through the Rust global allocator since boot.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_peak_bytes() -> usize {
        nros_platform::heap_stats::peak()
    }

    /// Bytes currently outstanding from the platform's *unified* heap — the
    /// true figure spanning both the Rust global allocator and the C side
    /// (zenoh-pico etc.), where the port owns one shared kernel heap. `0` if
    /// the port does not instrument heap usage.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_platform_used_bytes() -> usize {
        unsafe { nros_platform_heap_used_bytes() }
    }

    /// Total managed heap size in bytes (used + free) reported by the
    /// platform, or `0` if unknown.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_total_bytes() -> usize {
        unsafe { nros_platform_heap_total_bytes() }
    }
}

// Global allocator: NOT defined here. RFC-0034 D6 still holds — Rust
// `Box`/`Vec` route through the platform vtable (`nros_platform_alloc` → the
// port's kernel allocator: FreeRTOS `pvPortMalloc`, Zephyr `k_malloc`,
// ThreadX `tx_byte_allocate`, …) so the C/C++ API Rust heap and zenoh-pico's
// C-side `z_malloc` share one funnel and one arena. What changed in phase-341
// W8.c is WHO installs the lang item.
//
// This crate used to define its own `#[global_allocator]` under the gate
// `all(global-allocator, not(std))` — the SAME gate `nros-platform` uses for
// its own, reaching the same heap by a different route (a direct `extern "C"`
// rather than `<ConcretePlatform as PlatformAlloc>`). `nros-c` deps
// `nros-platform` non-optionally, so an image enabling both features got a
// duplicate lang item, and nothing but a manifest comment stood in the way.
// `global-allocator` now forwards to `nros-platform/global-allocator`, which
// makes two impossible instead of merely discouraged.
//
// The `extern crate` is load-bearing, not decorative. A `#[global_allocator]`
// only reaches the image if the crate DEFINING it is actually linked, and a
// dependency this crate never names in code is dropped before that — the same
// DCE class as the backend `FORCE_LINK` statics. Without this line,
// `nros-c --features platform-threadx,alloc` fails with "no global memory
// allocator found" while `cargo tree` happily shows
// `nros-platform feature "global-allocator"` enabled. `alloc-stats` masked it
// by giving the crate an unrelated reason to be referenced.
#[cfg(all(feature = "global-allocator", not(feature = "std")))]
extern crate nros_platform;
// Minimal panic handler for the no_std C/C++ API staticlib when no other
// panic strategy is linked (no `std`, no `panic-halt`). The Rust API path
// defers to the platform crate / zephyr-lang-rust's panic_handler; the
// standalone C/C++ staticlib needs its own. A halt+reboot would be ideal
// but needs port-specific config (e.g. Zephyr's k_panic + CONFIG_ASSERT_
// VERBOSE); looping is the safest no_std-compatible default.
// phase-341 W8.b / issue 0471 — gated on `panic-spin`, NOT on
// `global-allocator`. Asking for a panic handler no longer means asking for a
// heap; the `platform-*` features select both, so they stay unified per
// platform without being the same switch.
#[cfg(all(
    feature = "panic-spin",
    not(feature = "std"),
    not(feature = "panic-halt")
))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

// critical-section impl backed by the platform vtable
// (`nros_platform_critical_section_{acquire,release}`). Phase 248 —
// platform-agnostic: the concrete IRQ-mask logic lives in the platform
// shim behind the vtable, not here. Kept outside the allocator module so
// `std` builds (e.g. Zephyr native_sim) also provide the backend for Rust
// dependencies (DDS + portable-atomic require a registered impl).
#[cfg(feature = "critical-section")]
mod platform_critical_section {
    unsafe extern "C" {
        fn nros_platform_critical_section_acquire() -> u32;
        fn nros_platform_critical_section_release(token: u32);
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn acquire_key() -> critical_section::RawRestoreState {
        unsafe { nros_platform_critical_section_acquire() }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn acquire_key() -> critical_section::RawRestoreState {
        let _ = unsafe { nros_platform_critical_section_acquire() };
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn release_key(token: critical_section::RawRestoreState) {
        unsafe { nros_platform_critical_section_release(token) }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn release_key(_token: critical_section::RawRestoreState) {
        unsafe { nros_platform_critical_section_release(0) }
    }

    struct PlatformCs;
    critical_section::set_impl!(PlatformCs);

    unsafe impl critical_section::Impl for PlatformCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            unsafe { acquire_key() }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            unsafe { release_key(token) }
        }
    }
}

// ── Modules ─────────────────────────────────────────────────────────────

// Validation macros (must precede all other modules)
#[macro_use]
mod macros;

// Build-time configurable constants (generated by build.rs from NROS_* env vars)
#[cfg(all(not(cbindgen), feature = "rmw-cffi"))]
pub(crate) mod config;

// Backend-independent modules (always available)
mod cdr;
mod clock;
mod constants;
mod error;
mod log;
mod opaque_sizes;
mod parameter;
mod platform;
mod qos;
mod transport;
mod util;

// Phase 241.D3-rev — `#[used]` anchor over this crate's `#[no_mangle]` C surface, so
// the entry points survive DCE when `nros-c` is bundled as an rlib by the `nros-cpp`
// umbrella (a C++ binary may call a C-API fn the C++ FFI never references). Generated
// by build.rs (ungated entry points only). See `generate_c_surface_anchor`.
include!(concat!(env!("OUT_DIR"), "/c_surface_anchor.rs"));

// Issue 0360 — the archive half of the variant stamp. Defines
// `nros_config_variant_<feature_slug>`, which the generated
// `nros_config_generated.h` declares and anchors, so compiling against one
// feature set and linking another fails at LINK instead of overflowing
// consumer buffers at runtime.
include!(concat!(env!("OUT_DIR"), "/variant_symbol.rs"));

pub use cdr::*;
pub use clock::*;
pub use constants::*;
pub use error::*;
pub use parameter::*;
pub use qos::*;
pub use transport::*;

// Backend-dependent modules (require an RMW backend)
// These reference support/node types which depend on the active backend.
// Features pass through to `nros`, which provides the concrete types via
// `nros::internals::Rmw*` type aliases.

// For cbindgen: unconditional module declarations so cbindgen can find
// all #[repr(C)] types. cbindgen sets cfg(cbindgen)=true automatically.
#[cfg(cbindgen)]
mod action;
#[cfg(cbindgen)]
mod config;
#[cfg(cbindgen)]
mod event;
#[cfg(cbindgen)]
mod executor;
#[cfg(cbindgen)]
mod guard_condition;
#[cfg(cbindgen)]
mod lifecycle;
#[cfg(cbindgen)]
mod node;
#[cfg(cbindgen)]
mod publisher;
#[cfg(cbindgen)]
mod service;
#[cfg(cbindgen)]
mod subscription;
#[cfg(cbindgen)]
mod support;
#[cfg(cbindgen)]
mod timer;

// For actual compilation: feature-gated modules
#[cfg(not(cbindgen))]
macro_rules! rmw_modules {
    ($(mod $mod:ident;)*) => {
        $(
            #[cfg(feature = "rmw-cffi")]
            mod $mod;
            #[cfg(feature = "rmw-cffi")]
            pub use $mod::*;
        )*
    };
}

#[cfg(not(cbindgen))]
rmw_modules! {
    mod action;
    mod event;
    mod executor;
    mod guard_condition;
    mod lifecycle;
    mod node;
    mod publisher;
    mod service;
    mod subscription;
    mod support;
    mod timer;
}

// ---------------------------------------------------------------------------
// phase-341 W8.e / issue 0471 — capabilities REQUIRE the heap / the standard
// library, they do not enable it. Turning `alloc` or `std` on for the user
// silently changes what their firmware image is; naming the feature they must
// add does not.
// ---------------------------------------------------------------------------
#[cfg(all(
    feature = "param-services",
    not(any(feature = "alloc", feature = "std"))
))]
compile_error!("`param-services` allocates: add \"alloc\" to this crate's features");
#[cfg(all(
    feature = "lifecycle-services",
    not(any(feature = "alloc", feature = "std"))
))]
compile_error!("`lifecycle-services` allocates: add \"alloc\" to this crate's features");
