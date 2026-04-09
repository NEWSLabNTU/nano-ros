//! DWT-based monotonic clock for STM32F4.
//!
//! Uses the Cortex-M4 DWT cycle counter for hardware-accurate timing.
//! The software clock is updated from DWT cycles via `update_from_dwt()`,
//! which must be called periodically (at least once per ~25.6s at 168 MHz).

use core::sync::atomic::{AtomicU32, Ordering};

/// Global millisecond counter (lower 32 bits).
static CLOCK_MS_LO: AtomicU32 = AtomicU32::new(0);
/// Global millisecond counter (upper 32 bits).
static CLOCK_MS_HI: AtomicU32 = AtomicU32::new(0);

/// DWT ticks per millisecond (set during init).
static mut TICKS_PER_MS: u32 = 168_000; // default for 168 MHz
/// Last DWT cycle count when clock was updated.
static mut LAST_DWT_TICK: u32 = 0;

/// Initialize the clock with the system clock frequency.
pub fn init(sysclk_hz: u32) {
    unsafe {
        TICKS_PER_MS = sysclk_hz / 1000;
        LAST_DWT_TICK = cortex_m::peripheral::DWT::cycle_count();
    }
}

/// Update the software clock from DWT hardware counter.
///
/// Must be called periodically (at least once per ~25.6 seconds at 168 MHz)
/// to avoid missing DWT counter wraps.
pub fn update_from_dwt() {
    unsafe {
        let now = cortex_m::peripheral::DWT::cycle_count();
        let elapsed_ticks = now.wrapping_sub(LAST_DWT_TICK);
        let elapsed_ms = elapsed_ticks as u64 / TICKS_PER_MS as u64;
        if elapsed_ms > 0 {
            advance_clock_ms(elapsed_ms);
            LAST_DWT_TICK = LAST_DWT_TICK.wrapping_add((elapsed_ms as u32) * TICKS_PER_MS);
        }
    }
}

/// Get the current time in milliseconds.
#[inline]
pub fn clock_ms() -> u64 {
    loop {
        let hi1 = CLOCK_MS_HI.load(Ordering::Relaxed);
        let lo = CLOCK_MS_LO.load(Ordering::Relaxed);
        let hi2 = CLOCK_MS_HI.load(Ordering::Relaxed);
        if hi1 == hi2 {
            return ((hi1 as u64) << 32) | (lo as u64);
        }
    }
}

/// Advance the clock by the specified number of milliseconds.
#[inline]
pub fn advance_clock_ms(ms: u64) {
    let old = clock_ms();
    let new = old.wrapping_add(ms);
    CLOCK_MS_HI.store((new >> 32) as u32, Ordering::Relaxed);
    CLOCK_MS_LO.store(new as u32, Ordering::Relaxed);
}
