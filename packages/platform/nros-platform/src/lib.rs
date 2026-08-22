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

// Phase 212.N.1 — the Board trait family lives in `board/` (was a
// flat `board.rs`); `BoardConfig` + `BoardTransportConfig` stay at
// the crate root for back-compat. New 212.N consumers reach the
// full surface (`Board`, `BoardInit`, `BoardEntry`, …) through
// `nros_platform::board::*`.
pub use board::{
    Board, BoardConfig, BoardEntry, BoardExit, BoardInit, BoardPrint, BoardTransportConfig,
    DeployOverlay, DispatchStrategy, EmbassyBoardEntry, NetworkWait, NodeDispatchRuntime,
    NullNodeRuntime, PriorityDirection, RticBoardEntry, RuntimeCtx, RuntimeError, SignaledCallback,
    TierSpec, TierSpinGap, TransportBringup, boot_tier_index, freertos_priority_for,
    posix_nice_for, threadx_priority_for,
};
// Phase 313 W1 (issue #0243) — the deprecated `NodeRuntime` crate-root alias is
// removed; consumers use `NodeDispatchRuntime`.
// Phase 212.N.2 — `NetworkError` is the return type any external
// `NetworkWait` impl carries, so it needs to be reachable at the
// crate root. The `board` module stays private; this re-export keeps
// the boundary clean.
pub use board::network::NetworkError;

// Phase 129.C.3.b — `NET_*` constants exported unconditionally
// (see `resolve.rs`). `ConcretePlatform` keeps its feature gate
// because the type alias still needs a concrete platform crate
// linked in.
pub use resolve::{NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE};

#[cfg(any(
    feature = "platform-posix",
    feature = "platform-cffi",
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32-qemu",
    feature = "platform-nuttx",
    feature = "platform-freertos",
    feature = "platform-threadx",
    feature = "platform-zephyr",
))]
pub use resolve::ConcretePlatform;

// Re-export every trait from the split-out `nros-platform-api` crate so
// existing `use nros_platform::PlatformClock;` imports keep working.
pub use nros_platform_api::*;

// Link-graph anchor — relays an in-rlib `#[used]` static to the
// `_nros_force_link_cffi` symbol that lives in `nros-platform-cffi`.
// Downstream crates (`nros-rmw-zenoh`, the C/C++ FFI) reference
// `__FORCE_LINK_CFFI` from their own `#[used]` static, which chains
// up through this crate to cffi and keeps the `libnros_platform_posix.a`
// static lib in the final link. Without the chain, rustc elides the
// cffi rlib and every `nros_platform_*` C symbol is unresolved.
#[cfg(feature = "platform-posix")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_CFFI: extern "C" fn() = nros_platform_cffi::_nros_force_link_cffi;

// ============================================================================
// Phase 248 C7 — Zephyr platform helper (relocated from `nros::platform::zephyr`)
// ============================================================================
/// Zephyr-specific platform helpers.
///
/// On Zephyr's `native_sim`, the default network interface is assigned an IPv4
/// address at boot, but the underlying TAP link reports `net_if_is_up() == false`
/// for ~100–200 ms until the host side is ready. Opening a zenoh session before
/// that returns `TransportError::ConnectionFailed`. Call [`zephyr::wait_network`]
/// before `Executor::open`. Mirrors the `nros_platform_zephyr_wait_network()` C
/// helper the C/C++ examples use; the symbol is RMW-independent (defined in
/// `nros-platform-zephyr`, compiled in every RMW build). Equivalent to
/// `nros-board-zephyr`'s `ZephyrBoard::wait_link_up`.
#[cfg(feature = "platform-zephyr")]
pub mod zephyr {
    unsafe extern "C" {
        fn nros_platform_zephyr_wait_network(timeout_ms: i32) -> i32;
    }

    /// Block until the default Zephyr network interface is operational, or the
    /// timeout expires. `Ok(())` on link-up, `Err(())` on timeout.
    pub fn wait_network(timeout_ms: i32) -> Result<(), ()> {
        // SAFETY: `nros_platform_zephyr_wait_network` has no preconditions beyond
        // being called from a Zephyr thread context — always true in a Zephyr app.
        let ret = unsafe { nros_platform_zephyr_wait_network(timeout_ms) };
        if ret == 0 { Ok(()) } else { Err(()) }
    }
}

// ============================================================================
// Phase 71.27 — opt-in `#[global_allocator]`
// ============================================================================
//
// On bare-metal / RTOS targets DDS + heapless futures need a real
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

// phase-361 W8.c / issue 0594 — this is the ONE `#[global_allocator]` in the
// tree. `nros-c` used to define a second one under an identical gate, reaching
// the same heap by a different route (a direct `extern "C" nros_platform_alloc`
// rather than the trait), and the two were kept apart only by a manifest
// comment: `nros-c` deps `nros-platform` non-optionally, so any image that
// enabled both features got a duplicate lang item. `nros-c/global-allocator`
// now forwards here, which makes the duplication impossible rather than
// merely discouraged — cargo unifies one crate's one feature into one unit.
//
// This adapter covers BOTH link shapes, which the `nros-c` copy did not:
// every `platform-*` feature resolves `ConcretePlatform` to `CffiPlatform`
// (see `resolve.rs`), whose `PlatformAlloc` impl IS `nros_platform_alloc`, and
// the bare-metal Rust crates (mps2-an385, stm32f4, esp32-qemu) reach their own
// arena through the same trait. One API, one arena, per RFC-0034 D6.
#[cfg(all(feature = "global-allocator", not(feature = "std")))]
mod global_allocator {
    use core::{
        alloc::{GlobalAlloc, Layout},
        ffi::c_void,
    };

    use crate::ConcretePlatform;
    use nros_platform_api::PlatformAlloc;

    /// Alignment the platform ABI guarantees. `nros_platform_alloc` has no
    /// alignment parameter, and every port behind it returns memory aligned
    /// for the widest scalar — 8 bytes on every target nano-ros builds for.
    const PLATFORM_ALIGN: usize = 8;

    /// `GlobalAlloc` adapter over `<ConcretePlatform as PlatformAlloc>`.
    pub struct PlatformGlobalAllocator;

    unsafe impl GlobalAlloc for PlatformGlobalAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // phase-361 W8.c — an over-aligned request FAILS rather than
            // silently returning under-aligned memory. The ABI cannot express
            // alignment, so the honest answer to `align > 8` is null, which
            // routes the caller into `handle_alloc_error`. The previous
            // `let _ = layout.align();` produced UB no build could see;
            // `zpico-alloc` already answered the same question with null.
            if layout.align() > PLATFORM_ALIGN {
                return core::ptr::null_mut();
            }
            let p = <ConcretePlatform as PlatformAlloc>::alloc(layout.size()) as *mut u8;
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                super::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
            <ConcretePlatform as PlatformAlloc>::dealloc(ptr as *mut c_void);
            #[cfg(feature = "alloc-stats")]
            super::heap_stats::STATS.on_dealloc(_layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: PlatformGlobalAllocator = PlatformGlobalAllocator;
}

// phase-361 W8.c — the Rust-footprint heap counter, moved here with the
// allocator it instruments. It counts only what passes through the
// `#[global_allocator]`; the C side's direct `nros_platform_alloc` traffic
// (zenoh-pico's `z_malloc` etc.) is not seen, so it under-reports true heap
// pressure. The *unified* figure is the platform's own
// `nros_platform_heap_used_bytes` (RFC-0034 D7).
//
// No `#[no_mangle]` here: the C names (`nros_heap_used_bytes` …) belong to the
// C/C++ API surface and stay exported by `nros-c` / `nros-cpp`, which read
// these accessors. A pure-Rust image gets the counter without gaining C
// symbols it never asked for.
#[cfg(feature = "alloc-stats")]
pub mod heap_stats {
    use core::sync::atomic::{AtomicUsize, Ordering};

    /// Bytes outstanding through the Rust global allocator, and the high-water
    /// mark since boot. `Relaxed` throughout — this is instrumentation, and no
    /// other state is ordered against it.
    pub struct HeapStats {
        used_bytes: AtomicUsize,
        peak_bytes: AtomicUsize,
    }

    impl HeapStats {
        /// Create a zeroed counter. `const` so it can back a `static`.
        pub const fn new() -> Self {
            Self {
                used_bytes: AtomicUsize::new(0),
                peak_bytes: AtomicUsize::new(0),
            }
        }

        /// Record a successful allocation of `size` bytes and update the peak.
        #[inline]
        pub fn on_alloc(&self, size: usize) {
            let used = self.used_bytes.fetch_add(size, Ordering::Relaxed) + size;
            let _ = self.peak_bytes.fetch_max(used, Ordering::Relaxed);
        }

        /// Record a deallocation of `size` bytes.
        #[inline]
        pub fn on_dealloc(&self, size: usize) {
            self.used_bytes.fetch_sub(size, Ordering::Relaxed);
        }

        /// Bytes currently outstanding.
        #[inline]
        pub fn used(&self) -> usize {
            self.used_bytes.load(Ordering::Relaxed)
        }

        /// Peak outstanding bytes since boot.
        #[inline]
        pub fn peak(&self) -> usize {
            self.peak_bytes.load(Ordering::Relaxed)
        }
    }

    impl Default for HeapStats {
        fn default() -> Self {
            Self::new()
        }
    }

    pub static STATS: HeapStats = HeapStats::new();

    /// Bytes currently outstanding through the Rust global allocator.
    #[inline]
    pub fn used() -> usize {
        STATS.used()
    }

    /// Peak outstanding bytes through the Rust global allocator since boot.
    #[inline]
    pub fn peak() -> usize {
        STATS.peak()
    }
}
