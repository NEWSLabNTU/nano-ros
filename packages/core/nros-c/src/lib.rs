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

#[cfg(feature = "alloc")]
extern crate alloc;

// FreeRTOS global allocator: wraps pvPortMalloc/vPortFree for alloc on no_std.
// FreeRTOS heap_4 returns 8-byte aligned pointers, sufficient for all nros types.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-freertos"))]
mod freertos_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn pvPortMalloc(size: u32) -> *mut core::ffi::c_void;
        fn vPortFree(ptr: *mut core::ffi::c_void);
    }

    struct FreeRtosAllocator;

    unsafe impl GlobalAlloc for FreeRtosAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { pvPortMalloc(layout.size() as u32) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { vPortFree(ptr as *mut core::ffi::c_void) }
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

    unsafe extern "C" {
        fn k_malloc(size: usize) -> *mut core::ffi::c_void;
        fn k_free(ptr: *mut core::ffi::c_void);
    }

    struct ZephyrAllocator;

    unsafe impl GlobalAlloc for ZephyrAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { k_malloc(layout.size()) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { k_free(ptr as *mut core::ffi::c_void) }
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

    // critical-section impl backed by Zephyr's nros_zephyr_irq_lock /
    // nros_zephyr_irq_unlock. dust-dds + portable-atomic require this on
    // no_std targets when zephyr-lang-rust isn't linked in.
    unsafe extern "C" {
        fn nros_zephyr_irq_lock() -> u32;
        fn nros_zephyr_irq_unlock(key: u32);
    }

    struct ZephyrCs;
    critical_section::set_impl!(ZephyrCs);

    unsafe impl critical_section::Impl for ZephyrCs {
        unsafe fn acquire() -> critical_section::RawRestoreState {
            unsafe { nros_zephyr_irq_lock() }
        }

        unsafe fn release(token: critical_section::RawRestoreState) {
            unsafe { nros_zephyr_irq_unlock(token) }
        }
    }
}

// ThreadX global allocator: wraps z_malloc/z_free which delegate to
// tx_byte_allocate/tx_byte_release via nros-platform-threadx.
#[cfg(all(feature = "alloc", not(feature = "std"), feature = "platform-threadx"))]
mod threadx_alloc {
    use core::alloc::{GlobalAlloc, Layout};

    unsafe extern "C" {
        fn z_malloc(size: usize) -> *mut core::ffi::c_void;
        fn z_free(ptr: *mut core::ffi::c_void);
    }

    struct ThreadXAllocator;

    unsafe impl GlobalAlloc for ThreadXAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { z_malloc(layout.size()) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            unsafe { z_free(ptr as *mut core::ffi::c_void) }
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
#[cfg(all(not(cbindgen), any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")))]
pub(crate) mod config;

// Backend-independent modules (always available)
mod cdr;
mod clock;
mod constants;
mod error;
mod opaque_sizes;
mod parameter;
mod platform;
mod qos;
mod util;

pub use cdr::*;
pub use clock::*;
pub use constants::*;
pub use error::*;
pub use parameter::*;
pub use qos::*;

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
            #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
            mod $mod;
            #[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
            pub use $mod::*;
        )*
    };
}

#[cfg(not(cbindgen))]
rmw_modules! {
    mod action;
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
