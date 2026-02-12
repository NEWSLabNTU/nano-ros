//! DWT-based monotonic clock for STM32F4
//!
//! Uses the Cortex-M4 DWT cycle counter for hardware-accurate timing.
//! The software clock is updated from DWT cycles in the poll callback.
//!
//! Implements zenoh-pico clock symbols directly (`z_clock_now`, `z_clock_elapsed_*`,
//! `z_clock_advance_*`) plus `smoltcp_clock_now_ms` for the transport crate.

use core::sync::atomic::{AtomicU32, Ordering};
use smoltcp::time::Instant;

/// Global millisecond counter (lower 32 bits)
static CLOCK_MS_LO: AtomicU32 = AtomicU32::new(0);
/// Global millisecond counter (upper 32 bits)
static CLOCK_MS_HI: AtomicU32 = AtomicU32::new(0);

/// DWT ticks per millisecond (set during init)
static mut TICKS_PER_MS: u32 = 168_000; // default for 168 MHz
/// Last DWT cycle count when clock was updated
static mut LAST_DWT_TICK: u32 = 0;

/// Initialize the clock with the system clock frequency
pub fn init(sysclk_hz: u32) {
    unsafe {
        TICKS_PER_MS = sysclk_hz / 1000;
        LAST_DWT_TICK = cortex_m::peripheral::DWT::cycle_count();
    }
}

/// Update the software clock from DWT hardware counter
///
/// Must be called periodically (at least once per ~25.6 seconds at 168MHz)
/// to avoid missing DWT counter wraps.
pub fn update_from_dwt() {
    unsafe {
        let now = cortex_m::peripheral::DWT::cycle_count();
        let elapsed_ticks = now.wrapping_sub(LAST_DWT_TICK);
        let elapsed_ms = elapsed_ticks as u64 / TICKS_PER_MS as u64;
        if elapsed_ms > 0 {
            advance_clock_ms(elapsed_ms);
            // Advance LAST_DWT_TICK by the consumed ticks (not just setting to now,
            // to preserve sub-millisecond accuracy)
            LAST_DWT_TICK = LAST_DWT_TICK.wrapping_add((elapsed_ms as u32) * TICKS_PER_MS);
        }
    }
}

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
fn set_clock_ms(ms: u64) {
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
#[allow(dead_code)]
pub fn now() -> Instant {
    Instant::from_millis(clock_ms() as i64)
}

// ============================================================================
// FFI exports — zenoh-pico clock symbols
// ============================================================================

/// z_clock_t z_clock_now(void)
#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> u64 {
    clock_ms()
}

/// unsigned long z_clock_elapsed_us(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_us(time: *const u64) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let elapsed_ms = clock_ms().wrapping_sub(start);
    (elapsed_ms * 1000) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_ms(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_ms(time: *const u64) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    clock_ms().wrapping_sub(start) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_s(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_s(time: *const u64) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let elapsed_ms = clock_ms().wrapping_sub(start);
    (elapsed_ms / 1000) as core::ffi::c_ulong
}

/// void z_clock_advance_us(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_us(clock: *mut u64, duration: core::ffi::c_ulong) {
    unsafe {
        *clock += (duration as u64).div_ceil(1000);
    }
}

/// void z_clock_advance_ms(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_ms(clock: *mut u64, duration: core::ffi::c_ulong) {
    unsafe {
        *clock += duration as u64;
    }
}

/// void z_clock_advance_s(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_s(clock: *mut u64, duration: core::ffi::c_ulong) {
    unsafe {
        *clock += duration as u64 * 1000;
    }
}

// ============================================================================
// FFI export — transport crate needs this for smoltcp timestamping
// ============================================================================

/// Get current time in milliseconds (called by nano-ros-transport-smoltcp's bridge)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
