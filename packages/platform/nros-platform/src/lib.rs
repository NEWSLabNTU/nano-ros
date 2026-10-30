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
// flat `board.rs`); `BoardConfig` stays at
// the crate root for back-compat. New 212.N consumers reach the
// full surface (`Board`, `BoardInit`, `BoardEntry`, …) through
// `nros_platform::board::*`.
pub use board::{
    Board, BoardConfig, BoardEntry, BoardExit, BoardInit, BoardPrint, DeployOverlay,
    DispatchStrategy, LinkKind, NodeDispatchRuntime, NullNodeRuntime, PriorityDirection,
    RticBoardEntry, RuntimeCtx, RuntimeError, SignaledCallback, TierSpec, TierSpinGap,
    boot_tier_index, freertos_priority_for, posix_nice_for, threadx_priority_for,
};
// Phase 313 W1 (issue #0243) — the deprecated `NodeRuntime` crate-root alias is
// removed; consumers use `NodeDispatchRuntime`.
// Phase 212.N.2 — `NetworkError` is the error a board's link-up wait
// reports, so it needs to be reachable at the
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

// issue 1123 — the `nros_log` sink list, reachable from a crate the ENTRY
// already declares.
//
// `nros_platform_cffi::log::init_default()` is the one funnel that publishes a
// sink list (issue 0710 moved it there because the ABI it speaks is a
// DEPENDENCY, not a feature). Every board crate calls it in its own boot path —
// but the Zephyr pure-Rust entry is a MACRO, not a board method, and neither
// the Zephyr example leaves (`examples/zephyr/rust/*`) nor `nros` itself dep
// `nros-platform-cffi`, so no expansion could name it. They all dep THIS crate
// (`nros-platform = { features = ["platform-zephyr"] }`), and every
// `platform-*` feature already pulls `nros-platform-cffi` in, so the re-export
// costs nothing and adds no link-time requirement that the feature did not
// already carry.
//
// Deliberately NOT giving the leaves a `nros-platform-cffi` dep of their own:
// that pushes a link-time requirement onto every consumer, which is the shape
// issue 0710 rejected.
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
pub use nros_platform_cffi::log;

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

/// Issue 1732 — a termination signal ENDS a hosted image's spin, so the boot
/// funnel can close the RMW session before the process exits.
///
/// The one spelling for every hosted funnel: the Rust board
/// (`nros-board-linux`, both its single-executor and its tiered path), the
/// `nros::main!` hosted spin loops, and the native C/C++ runners in
/// `nros-cpp`. The guard itself is C, in the POSIX port
/// (`nros_posix_install_termination_guard` in `nros-platform-posix`), because
/// that is the one translation unit every hosted image already links.
///
/// The contract, all of it in the port:
///
/// * installed only over the DEFAULT disposition of SIGTERM / SIGINT — an
///   application's own handler and `SIG_IGN` are left alone;
/// * the handler only sets a flag; nothing is torn down in a signal frame;
/// * a SECOND signal ends the image by the default action, so a wedged
///   teardown is still stoppable without SIGKILL.
///
/// On a build without the POSIX port nothing is installed and
/// [`requested`](termination::requested) is always `false`, so a loop that
/// polls it costs a constant.
pub mod termination {
    #[cfg(feature = "platform-posix")]
    unsafe extern "C" {
        fn nros_posix_install_termination_guard() -> i32;
        fn nros_posix_termination_requested() -> i32;
    }

    /// Install the guard; returns how many of SIGTERM / SIGINT carry it (0 when
    /// the application already handles both, or on a non-POSIX build).
    /// Idempotent. Call it before the session opens, so a signal that arrives
    /// during registration is not lost.
    pub fn install_guard() -> usize {
        #[cfg(feature = "platform-posix")]
        {
            // SAFETY: plain C call; `sigaction` on the process's own handlers.
            let n = unsafe { nros_posix_install_termination_guard() };
            if n > 0 { n as usize } else { 0 }
        }
        #[cfg(not(feature = "platform-posix"))]
        {
            0
        }
    }

    /// `true` once a guarded termination signal has arrived. A spin loop that
    /// sees it returns, and its funnel closes the session.
    pub fn requested() -> bool {
        #[cfg(feature = "platform-posix")]
        {
            // SAFETY: reads a `sig_atomic_t`.
            unsafe { nros_posix_termination_requested() != 0 }
        }
        #[cfg(not(feature = "platform-posix"))]
        {
            false
        }
    }
}

/// The kernel's deadline reservation on the calling thread (issue 2018).
///
/// The reservation is C, in the POSIX port
/// (`nros_posix_apply_current_deadline` in `nros-platform-posix`), for the
/// reason [`termination`] gives. It is reached only on a build that selected
/// that port. A `cfg(unix)` gate is not enough: Zephyr `native_sim`, and the
/// host tiers of ThreadX and FreeRTOS, target a Linux triple without the POSIX
/// port. Their port defines no `nros_posix_*` symbol, so a reference made the
/// linker take the POSIX `platform.o` from whichever archive carried one, and
/// every other symbol in it then collided with the image's own port
/// (issue 2032; issue 1776 had the same shape).
pub mod sched {
    use core::ffi::c_char;

    #[cfg(feature = "platform-posix")]
    unsafe extern "C" {
        fn nros_posix_apply_current_deadline(
            name: *const c_char,
            tier_class: *const c_char,
            budget_us: u64,
            period_us: u64,
            deadline_us: u64,
        ) -> i32;
    }

    /// Ask the kernel to reserve `budget_us` every `period_us` for the calling
    /// thread. Returns `true` when the kernel took it, and `false` otherwise.
    /// That covers a tier class that asks for no reservation, a refusal (no
    /// `CAP_SYS_NICE`), and a build whose port has no such kernel facility.
    /// On `false` the caller enforces the budget itself.
    ///
    /// # Safety
    ///
    /// `name` and `tier_class` are NUL-terminated strings or null.
    pub unsafe fn apply_current_deadline(
        name: *const c_char,
        tier_class: *const c_char,
        budget_us: u64,
        period_us: u64,
        deadline_us: u64,
    ) -> bool {
        #[cfg(feature = "platform-posix")]
        {
            // SAFETY: the caller's contract on the two strings.
            unsafe {
                nros_posix_apply_current_deadline(
                    name,
                    tier_class,
                    budget_us,
                    period_us,
                    deadline_us,
                ) == 1
            }
        }
        #[cfg(not(feature = "platform-posix"))]
        {
            let _ = (name, tier_class, budget_us, period_us, deadline_us);
            false
        }
    }
}

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
// phase-391 W3 — the rlsf arena behind Zephyr's C allocation funnel.
#[cfg(feature = "platform-zephyr")]
pub mod zephyr_heap;

#[cfg(feature = "platform-zephyr")]
pub mod zephyr {
    unsafe extern "C" {
        fn nros_platform_zephyr_wait_network(timeout_ms: i32) -> i32;
    }

    /// The default network interface did not come up within the timeout.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct NetworkTimeout;

    /// Block until the default Zephyr network interface is operational, or the
    /// timeout expires. `Ok(())` on link-up, `Err(NetworkTimeout)` on timeout.
    pub fn wait_network(timeout_ms: i32) -> Result<(), NetworkTimeout> {
        // SAFETY: `nros_platform_zephyr_wait_network` has no preconditions beyond
        // being called from a Zephyr thread context — always true in a Zephyr app.
        let ret = unsafe { nros_platform_zephyr_wait_network(timeout_ms) };
        if ret == 0 {
            Ok(())
        } else {
            Err(NetworkTimeout)
        }
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
            // phase-361 W8.c — never return under-aligned memory: the previous
            // `let _ = layout.align();` produced UB no build could see.
            //
            // An over-aligned request used to FAIL here (null, so
            // `handle_alloc_error`). phase-397 W2 made the executor's
            // `WakeCtx` `repr(align(64))` and put it in an `Arc`, so every
            // embedded image with an executor panicked at boot with "memory
            // allocation of 128 bytes failed" before registering a node
            // (S32K344, Zephyr 4.4). The ABI still cannot express alignment,
            // so an over-aligned block is carved out of a larger one instead
            // (`over_align`).
            let p = if layout.align() > PLATFORM_ALIGN {
                match super::over_align::raw_size(layout) {
                    // SAFETY: `raw_size` sized the raw block for `place`.
                    Some(raw_size) => unsafe {
                        super::over_align::place(
                            <ConcretePlatform as PlatformAlloc>::alloc(raw_size) as *mut u8,
                            layout,
                        )
                    },
                    None => core::ptr::null_mut(),
                }
            } else {
                <ConcretePlatform as PlatformAlloc>::alloc(layout.size()) as *mut u8
            };
            #[cfg(feature = "alloc-stats")]
            if !p.is_null() {
                super::heap_stats::STATS.on_alloc(layout.size());
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            let raw = if layout.align() > PLATFORM_ALIGN {
                // SAFETY: `ptr` came from the over-aligned branch of `alloc`
                // (same layout), which stored the raw block below it.
                unsafe { super::over_align::raw_of(ptr) }
            } else {
                ptr
            };
            <ConcretePlatform as PlatformAlloc>::dealloc(raw as *mut c_void);
            #[cfg(feature = "alloc-stats")]
            super::heap_stats::STATS.on_dealloc(layout.size());
        }
    }

    #[global_allocator]
    static ALLOCATOR: PlatformGlobalAllocator = PlatformGlobalAllocator;
}

/// An over-aligned block inside an 8-aligned one (the global allocator's
/// `align > 8` path). The raw block is `size + align` bytes; the returned
/// pointer is the first `align` boundary at least one `usize` past its start,
/// and that `usize` just below it keeps the raw pointer for `dealloc`.
///
/// Room: the raw pointer is 8-aligned and `align` is a power of two above 8,
/// so the gap to the boundary is a multiple of 8 in `8..=align`: the header
/// fits in it and `gap + size <= align + size`.
#[cfg_attr(
    not(all(feature = "global-allocator", not(feature = "std"))),
    allow(dead_code)
)]
mod over_align {
    use core::alloc::Layout;

    const HEADER: usize = core::mem::size_of::<usize>();

    /// The raw block's size for `layout`, or `None` on overflow.
    pub(crate) fn raw_size(layout: Layout) -> Option<usize> {
        layout.size().checked_add(layout.align())
    }

    /// The aligned pointer inside `raw` (null stays null).
    ///
    /// # Safety
    /// `raw` is null or an 8-aligned block of `raw_size(layout)` bytes, and
    /// `layout.align() > 8`.
    pub(crate) unsafe fn place(raw: *mut u8, layout: Layout) -> *mut u8 {
        if raw.is_null() {
            return raw;
        }
        let align = layout.align();
        let start = raw as usize;
        let aligned = (start + HEADER + align - 1) & !(align - 1);
        let p = raw.wrapping_add(aligned - start);
        // SAFETY: `aligned - HEADER >= start`, and it is 8-aligned (both
        // `aligned` and `start` are), inside the raw block.
        unsafe { (p.sub(HEADER) as *mut usize).write(start) };
        p
    }

    /// The raw block `place` carved `p` out of.
    ///
    /// # Safety
    /// `p` came from `place`.
    pub(crate) unsafe fn raw_of(p: *mut u8) -> *mut u8 {
        // SAFETY: `place` wrote the raw pointer there.
        let start = unsafe { (p.sub(HEADER) as *const usize).read() };
        p.wrapping_sub(p as usize - start)
    }

    #[cfg(test)]
    mod tests {
        extern crate std;
        use super::*;
        use std::{vec, vec::Vec};

        #[test]
        fn every_offset_fits_and_round_trips() {
            for align in [16usize, 32, 64, 128] {
                for size in [1usize, 8, 63, 64, 128, 200] {
                    let layout = Layout::from_size_align(size, align).unwrap();
                    let n = raw_size(layout).unwrap();
                    // Every 8-aligned start an align-period can present.
                    let mut buf: Vec<u64> = vec![0; (n + 2 * align) / 8 + 1];
                    let base = buf.as_mut_ptr() as *mut u8;
                    for off in (0..align).step_by(8) {
                        let raw = base.wrapping_add(off);
                        let p = unsafe { place(raw, layout) };
                        assert_eq!(p as usize % align, 0);
                        assert!(p as usize - raw as usize >= HEADER);
                        assert!(p as usize + size <= raw as usize + n);
                        assert_eq!(unsafe { raw_of(p) }, raw);
                    }
                }
            }
        }

        #[test]
        fn null_stays_null() {
            let layout = Layout::from_size_align(128, 64).unwrap();
            assert!(unsafe { place(core::ptr::null_mut(), layout) }.is_null());
        }
    }
}

// phase-361 W8.c — the Rust-footprint heap counter, moved here with the
// allocator it instruments. It counts only what passes through the
// `#[global_allocator]`; the C side's direct `nros_platform_alloc` traffic
// (zenoh-pico's `z_malloc` etc.) is not seen, so it under-reports true heap
// pressure. The *unified* figure is the platform's own
// `nros_platform_heap_used_bytes` (RFC-0034 D7).
//
// No `#[no_mangle]` here: the C names (`rclc_heap_used_bytes` …) belong to the
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
