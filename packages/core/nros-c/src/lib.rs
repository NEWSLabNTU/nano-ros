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

// FreeRTOS global allocator (C/C++ API path). Phase 230 1d / RFC-0034 D6:
// routes through the platform ABI (`nros_platform_alloc` → `pvPortMalloc`)
// rather than calling `pvPortMalloc` directly, so Rust `Box`/`Vec` and
// zenoh-pico's C `z_malloc` share the one `nros_platform_alloc` funnel and
// the unified heap query (`nros_platform_heap_used_bytes`, 1b) is exact.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-freertos"))]
mod freertos_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct FreeRtosAllocator;

    unsafe impl GlobalAlloc for FreeRtosAllocator {
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
    static ALLOCATOR: FreeRtosAllocator = FreeRtosAllocator;
}

// Zephyr global allocator: wraps Zephyr's k_malloc/k_free, backed by
// CONFIG_HEAP_MEM_POOL_SIZE. Required for the C/C++ API path on Zephyr
// targets that don't bring zephyr-lang-rust's static_alloc with them
// (e.g. qemu_cortex_a9 with the DDS RMW backend). Phase 71.6.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-zephyr"))]
mod zephyr_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    // Phase 230 1d / RFC-0034 D6 — route through the platform ABI
    // (`nros_platform_alloc` → `k_malloc`) so the C/C++ API Rust heap shares
    // the funnel with zenoh-pico's C side.
    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct ZephyrAllocator;

    unsafe impl GlobalAlloc for ZephyrAllocator {
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
    static ALLOCATOR: ZephyrAllocator = ZephyrAllocator;

    // Minimal panic handler for the no_std + platform-zephyr build.
    // The Rust API path defers to zephyr-lang-rust's panic_handler; the
    // C/C++ API path needs its own. Halt + reboot via Zephyr's k_panic
    // would be ideal, but k_panic requires CONFIG_ASSERT_VERBOSE; just
    // looping is the safest no_std-compatible default.
    #[panic_handler]
    fn panic(_info: &core::panic::PanicInfo) -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
}

// critical-section impl backed by Zephyr's nros_zephyr_irq_lock /
// nros_zephyr_irq_unlock. Keep this outside the allocator module so native_sim
// std builds also provide the backend symbols for Rust dependencies.
#[cfg(feature = "platform-zephyr")]
mod zephyr_critical_section {
    unsafe extern "C" {
        fn nros_zephyr_irq_lock() -> u32;
        fn nros_zephyr_irq_unlock(key: u32);
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn acquire_irq_key() -> critical_section::RawRestoreState {
        unsafe { nros_zephyr_irq_lock() }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn acquire_irq_key() -> critical_section::RawRestoreState {
        let _ = unsafe { nros_zephyr_irq_lock() };
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    unsafe fn release_irq_key(token: critical_section::RawRestoreState) {
        unsafe { nros_zephyr_irq_unlock(token) }
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    unsafe fn release_irq_key(_token: critical_section::RawRestoreState) {
        unsafe { nros_zephyr_irq_unlock(0) }
    }

    struct ZephyrCs;
    critical_section::set_impl!(ZephyrCs);

    unsafe impl critical_section::Impl for ZephyrCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            unsafe { acquire_irq_key() }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            unsafe { release_irq_key(token) }
        }
    }
}

// ThreadX global allocator (C/C++ API path). Phase 230 1d / RFC-0034 D6 —
// route through the platform ABI directly (`nros_platform_alloc` →
// `tx_byte_allocate`). Previously called `z_malloc`, which on ThreadX is the
// alias TU forwarding to the same symbol; the direct call drops the
// indirection and keeps every owned-allocator platform on one funnel.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-threadx"))]
mod threadx_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn nros_platform_alloc(size: usize) -> *mut core::ffi::c_void;
        fn nros_platform_dealloc(ptr: *mut core::ffi::c_void);
    }

    struct ThreadXAllocator;

    unsafe impl GlobalAlloc for ThreadXAllocator {
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
    static ALLOCATOR: ThreadXAllocator = ThreadXAllocator;
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
