//! Hardware timer clock for ESP32-C3 (WiFi)
//!
//! Uses `esp_hal::time::Instant` for monotonic timestamps.
//! The hardware timer is authoritative — no software counter needed.
//!
//! Implements zenoh-pico clock symbols directly (`z_clock_now`, `z_clock_elapsed_*`,
//! `z_clock_advance_*`) plus `smoltcp_clock_now_ms` for the transport crate.

use esp_hal::time::Instant;

/// Get the current time as a smoltcp Instant
#[inline]
pub fn now() -> smoltcp::time::Instant {
    smoltcp::time::Instant::from_micros(
        Instant::now().duration_since_epoch().as_micros() as i64,
    )
}

/// Get the current time in milliseconds
#[inline]
pub fn clock_ms() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
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

/// Get current time in milliseconds (called by nano-ros-link-smoltcp's bridge)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}
