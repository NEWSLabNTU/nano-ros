//! XRCE-DDS platform symbols for QEMU MPS2-AN385.
//!
//! Provides the FFI symbols needed by XRCE-DDS on bare-metal:
//! - `uxr_millis()` / `uxr_nanos()` — called from `session.c`, `ping.c`
//!   (since `time.c` is skipped for bare-metal builds)
//! - `smoltcp_clock_now_ms()` — called from `xrce-smoltcp` transport
//!
//! Uses a software millisecond counter (same pattern as
//! `zpico-platform-mps2-an385/src/clock.rs`). Board crates update the
//! counter via [`set_clock_ms()`] from their timer/poll loop.
//!
//! # Comparison with zpico-platform-mps2-an385
//!
//! | zpico (zenoh-pico) | xrce (XRCE-DDS) |
//! |----|-----|
//! | ~55 symbols (memory, clock, RNG, sleep, threading, sockets, libc) | 3 symbols (uxr_millis, uxr_nanos, smoltcp_clock_now_ms) |

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};

// ============================================================================
// Software Clock
// ============================================================================

/// Global millisecond counter (lower 32 bits).
static CLOCK_MS_LO: AtomicU32 = AtomicU32::new(0);
/// Global millisecond counter (upper 32 bits).
static CLOCK_MS_HI: AtomicU32 = AtomicU32::new(0);

/// Get the current time in milliseconds.
#[inline]
pub fn clock_ms() -> u64 {
    // Read high, low, high again to handle wrap-around
    loop {
        let hi1 = CLOCK_MS_HI.load(Ordering::Relaxed);
        let lo = CLOCK_MS_LO.load(Ordering::Relaxed);
        let hi2 = CLOCK_MS_HI.load(Ordering::Relaxed);
        if hi1 == hi2 {
            return ((hi1 as u64) << 32) | (lo as u64);
        }
    }
}

/// Set the current time in milliseconds.
///
/// Board crates call this from their timer/poll loop to keep the
/// monotonic clock advancing.
#[inline]
pub fn set_clock_ms(ms: u64) {
    CLOCK_MS_HI.store((ms >> 32) as u32, Ordering::Relaxed);
    CLOCK_MS_LO.store(ms as u32, Ordering::Relaxed);
}

// ============================================================================
// FFI Exports — XRCE-DDS time symbols
// ============================================================================

/// `int64_t uxr_millis(void)` — current time in milliseconds.
///
/// Called by `session.c` and `ping.c` for timeout management.
/// Replaces `time.c` which is only compiled for POSIX builds.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_millis() -> i64 {
    clock_ms() as i64
}

/// `int64_t uxr_nanos(void)` — current time in nanoseconds.
///
/// Called by `session.c` for time synchronization.
/// Replaces `time.c` which is only compiled for POSIX builds.
#[unsafe(no_mangle)]
pub extern "C" fn uxr_nanos() -> i64 {
    (clock_ms() as i64).wrapping_mul(1_000_000)
}

// ============================================================================
// FFI Export — smoltcp transport clock
// ============================================================================

/// Get current time in milliseconds (called by `xrce-smoltcp`).
///
/// Same symbol name as zpico-smoltcp uses, so platform crates only
/// need one clock source for either transport backend.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
