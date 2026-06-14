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

// Phase 248 (C3.2) — the `cffi-xrce-c` `extern crate nros_rmw_xrce_cffi`
// and the `cffi-zenoh-cffi` `extern "C" { nros_rmw_zenoh_register }`
// forward-declaration are RETIRED along with their features. nros-c no
// longer pulls any concrete backend into `libnros_c.a`'s Rust graph: the
// `nros_rmw_<x>_register` C symbol is resolved at the final link step from
// the standalone `libnros_rmw_<x>.a` (board / `nano_ros_link_rmw()` on
// hosted targets; a sibling `nros-rmw-<x>-cffi-staticlib` cargo build on
// Zephyr's in-tree path).

#[cfg(feature = "alloc")]
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
#[cfg(feature = "alloc-stats")]
mod heap_stats {
    pub static STATS: zpico_alloc::HeapStats = zpico_alloc::HeapStats::new();

    // Canonical platform heap query (RFC-0034 D7). Resolved at the final
    // C-binary link step from the linked `nros-platform-<rtos>` cffi shim.
    unsafe extern "C" {
        fn nros_platform_heap_used_bytes() -> usize;
        fn nros_platform_heap_total_bytes() -> usize;
    }

    /// Bytes currently outstanding through the Rust global allocator.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_used_bytes() -> usize {
        STATS.used()
    }

    /// Peak outstanding bytes through the Rust global allocator since boot.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_heap_peak_bytes() -> usize {
        STATS.peak()
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

// Global allocator (C/C++ API path). RFC-0034 D6 — routes Rust
// `Box`/`Vec` through the platform vtable (`nros_platform_alloc` → the
// port's kernel allocator: FreeRTOS `pvPortMalloc`, Zephyr `k_malloc`,
// ThreadX `tx_byte_allocate`, …) so the C/C++ API Rust heap and
// zenoh-pico's C-side `z_malloc` share the one `nros_platform_alloc`
// funnel and the unified heap query (`nros_platform_heap_used_bytes`) is
// exact. Phase 248 — platform-agnostic: the concrete allocator lives
// behind the vtable, not in this crate. Installed when the build opts
// into `global-allocator` (RTOS ports whose kernel owns the unified heap
// and that don't bring an external allocator such as zephyr-lang-rust's
// static_alloc); `std` builds use the system allocator instead.
#[cfg(all(feature = "global-allocator", not(feature = "std")))]
mod platform_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct PlatformAllocator;

    unsafe impl GlobalAlloc for PlatformAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { nros_platform_alloc(layout.size()) as *mut u8 };
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                crate::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { nros_platform_dealloc(ptr as *mut core::ffi::c_void) }
            #[cfg(feature = "alloc-stats")]
            crate::heap_stats::STATS.on_dealloc(_layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: PlatformAllocator = PlatformAllocator;
}

// Minimal panic handler for the no_std C/C++ API staticlib when no other
// panic strategy is linked (no `std`, no `panic-halt`). The Rust API path
// defers to the platform crate / zephyr-lang-rust's panic_handler; the
// standalone C/C++ staticlib needs its own. A halt+reboot would be ideal
// but needs port-specific config (e.g. Zephyr's k_panic + CONFIG_ASSERT_
// VERBOSE); looping is the safest no_std-compatible default.
#[cfg(all(
    feature = "global-allocator",
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
