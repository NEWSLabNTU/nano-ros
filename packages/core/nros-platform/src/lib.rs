//! Unified platform abstraction traits for nros.
//!
//! This crate defines the backend-agnostic interface that platform
//! implementations (POSIX, Zephyr, FreeRTOS, bare-metal, etc.) must satisfy.
//! RMW backends consume these traits via thin shim crates that translate
//! RMW-specific C symbols (e.g., `z_clock_now`, `uxr_millis`) into calls
//! on the active platform implementation.
//!
//! # Trait hierarchy
//!
//! Capabilities are split into independent sub-traits so each RMW backend
//! can declare exactly what it needs:
//!
//! - [`PlatformClock`] — monotonic clock (required by all backends)
//! - [`PlatformAlloc`] — heap allocation (zenoh-pico only)
//! - [`PlatformSleep`] — sleep / delay (zenoh-pico only)
//! - [`PlatformYield`] — cooperative yield (zenoh-pico `socket_wait_event`)
//! - [`PlatformRandom`] — pseudo-random number generation (zenoh-pico only)
//! - [`PlatformTime`] — wall-clock time (zenoh-pico only)
//! - [`PlatformThreading`] — tasks, mutexes, condvars (multi-threaded platforms)
//!
//! # Compile-time resolution
//!
//! Exactly one platform feature must be enabled. The `ConcretePlatform`
//! type alias (gated on any `platform-*` feature) resolves to the active
//! backend, eliminating generic parameters.

#![no_std]

mod board;
mod resolve;

pub use board::BoardConfig;

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-cffi",
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32",
    feature = "platform-esp32-qemu",
    feature = "platform-nuttx",
    feature = "platform-freertos",
    feature = "platform-threadx",
    feature = "platform-zephyr",
))]
pub use resolve::{
    ConcretePlatform, NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

// Re-export every trait from the split-out `nros-platform-api` crate so
// existing `use nros_platform::PlatformClock;` imports keep working.
pub use nros_platform_api::*;

// ============================================================================
// Phase 71.27 — opt-in `#[global_allocator]`
// ============================================================================
//
// On bare-metal / RTOS targets dust-dds + heapless futures need a real
// heap. Each `nros-platform-*` crate already implements `PlatformAlloc`
// against its native heap (`pvPortMalloc` on FreeRTOS,
// `tx_byte_allocate` on ThreadX, `kmm_malloc` on NuttX,
// `k_malloc` on Zephyr, libc `malloc` on POSIX). This module promotes
// that trait impl into a `#[global_allocator]` so application crates
// don't have to write per-platform glue.
//
// Off by default — `platform-posix` users link against libstd's
// allocator. Enable via `nros-platform/global-allocator` in the
// example crate's `Cargo.toml` to wire it in.

#[cfg(all(feature = "global-allocator", not(feature = "std")))]
mod global_allocator {
    use core::alloc::{GlobalAlloc, Layout};
    use core::ffi::c_void;

    use crate::ConcretePlatform;
    use nros_platform_api::PlatformAlloc;

    /// `GlobalAlloc` adapter over `<ConcretePlatform as PlatformAlloc>`.
    pub struct PlatformGlobalAllocator;

    unsafe impl GlobalAlloc for PlatformGlobalAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // Most RTOS heaps don't honor alignment > sizeof(void*).
            // dust-dds's heaviest types are pointer-aligned, so this
            // matches the typical 8-byte heap alignment without
            // over-allocating. Callers that need larger alignment
            // (e.g. SIMD) should layer a custom allocator on top.
            let _ = layout.align();
            <ConcretePlatform as PlatformAlloc>::alloc(layout.size()) as *mut u8
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            <ConcretePlatform as PlatformAlloc>::dealloc(ptr as *mut c_void)
        }
    }

    #[global_allocator]
    static ALLOCATOR: PlatformGlobalAllocator = PlatformGlobalAllocator;
}
