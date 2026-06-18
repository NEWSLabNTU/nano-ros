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
    NullNodeRuntime, RticBoardEntry, RuntimeCtx, RuntimeError, SignaledCallback, TierSpec,
    TransportBringup, freertos_priority_for, posix_nice_for, threadx_priority_for,
};
// Phase 214.K.1 — backward-compat alias at the crate root. See
// `board::NodeRuntime` for the deprecation note.
#[allow(deprecated)]
pub use board::NodeRuntime;
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
    feature = "platform-orin-spe",
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

#[cfg(all(feature = "global-allocator", not(feature = "std")))]
mod global_allocator {
    use core::{
        alloc::{GlobalAlloc, Layout},
        ffi::c_void,
    };

    use crate::ConcretePlatform;
    use nros_platform_api::PlatformAlloc;

    /// `GlobalAlloc` adapter over `<ConcretePlatform as PlatformAlloc>`.
    pub struct PlatformGlobalAllocator;

    unsafe impl GlobalAlloc for PlatformGlobalAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // Most RTOS heaps don't honor alignment > sizeof(void*).
            // DDS's heaviest types are pointer-aligned, so this
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
