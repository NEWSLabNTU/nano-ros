//! Simple monotonic clock for bare-metal operation
//!
//! Provides a millisecond counter that can be updated from a timer interrupt
//! or busy-loop timing. This clock is used by both smoltcp and zenoh-pico.
//!
//! Note: Uses AtomicU32 for Cortex-M3 compatibility (no native 64-bit atomics).
//! This limits the clock range to ~49 days before wrapping.

use core::sync::atomic::{AtomicU32, Ordering};
use smoltcp::time::Instant;

/// Global millisecond counter (lower 32 bits)
static CLOCK_MS_LO: AtomicU32 = AtomicU32::new(0);
/// Global millisecond counter (upper 32 bits)
static CLOCK_MS_HI: AtomicU32 = AtomicU32::new(0);

/// Get the current time in milliseconds
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

/// Set the current time in milliseconds
#[inline]
pub fn set_clock_ms(ms: u64) {
    CLOCK_MS_HI.store((ms >> 32) as u32, Ordering::Relaxed);
    CLOCK_MS_LO.store(ms as u32, Ordering::Relaxed);
}

/// Advance the clock by the specified number of milliseconds
#[inline]
pub fn advance_clock_ms(ms: u64) {
    let old = clock_ms();
    set_clock_ms(old.wrapping_add(ms));
}

/// Get the current time as a smoltcp Instant
#[inline]
pub fn now() -> Instant {
    Instant::from_millis(clock_ms() as i64)
}

/// Busy-wait delay with clock advancement
///
/// This function busy-waits for approximately the specified number of
/// milliseconds while advancing the clock. The timing is approximate
/// and depends on CPU speed.
pub fn delay_ms(ms: u32) {
    // QEMU mps2-an385 runs at ~25MHz in emulation
    // This is a rough approximation - each iteration takes ~200ns
    for _ in 0..ms {
        for _ in 0..5000 {
            cortex_m::asm::nop();
        }
    }
    advance_clock_ms(ms as u64);
}

// ============================================================================
// FFI exports for zenoh-pico platform layer
// ============================================================================

/// Get current time in milliseconds (called by C platform layer)
#[no_mangle]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}

/// Set current time in milliseconds (called by C platform layer)
#[no_mangle]
pub extern "C" fn smoltcp_set_clock_ms(ms: u64) {
    set_clock_ms(ms);
}
