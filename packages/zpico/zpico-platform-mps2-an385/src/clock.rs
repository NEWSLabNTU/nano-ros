//! Simple monotonic clock for bare-metal operation
//!
//! Provides a millisecond counter that can be updated from a timer interrupt
//! or busy-loop timing. This clock is used by both smoltcp and zenoh-pico.
//!
//! Implements zenoh-pico clock symbols directly (`z_clock_now`, `z_clock_elapsed_*`,
//! `z_clock_advance_*`) plus `smoltcp_clock_now_ms` for the transport crate.
//!
//! Note: Uses AtomicU32 for Cortex-M3 compatibility (no native 64-bit atomics).

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

// ============================================================================
// FFI exports — zenoh-pico clock symbols
// ============================================================================
//
// z_clock_t is `void*` on bare-metal (zenoh-pico's void.h), so it is
// pointer-sized: 4 bytes on ARM32, 8 bytes on 64-bit targets. All clock
// functions must use `usize` (not `u64`) for the stored timestamp type
// to match the C ABI. The lower 32 bits of clock_ms() are sufficient
// (~49 days of uptime).

/// z_clock_t z_clock_now(void)
#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> usize {
    clock_ms() as usize
}

/// unsigned long z_clock_elapsed_us(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_us(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    let elapsed_ms = now.wrapping_sub(start);
    (elapsed_ms * 1000) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_ms(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_ms(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    now.wrapping_sub(start) as core::ffi::c_ulong
}

/// unsigned long z_clock_elapsed_s(z_clock_t *time)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_elapsed_s(time: *const usize) -> core::ffi::c_ulong {
    let start = unsafe { *time };
    let now = clock_ms() as usize;
    let elapsed_ms = now.wrapping_sub(start);
    (elapsed_ms / 1000) as core::ffi::c_ulong
}

/// void z_clock_advance_us(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_us(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add((duration as usize).div_ceil(1000));
    }
}

/// void z_clock_advance_ms(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_ms(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add(duration as usize);
    }
}

/// void z_clock_advance_s(z_clock_t *clock, unsigned long duration)
#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn z_clock_advance_s(clock: *mut usize, duration: core::ffi::c_ulong) {
    unsafe {
        *clock = (*clock).wrapping_add(duration as usize * 1000);
    }
}

// ============================================================================
// FFI export — transport crate needs this for smoltcp timestamping
// ============================================================================

/// Get current time in milliseconds (called by zpico-smoltcp's bridge)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
