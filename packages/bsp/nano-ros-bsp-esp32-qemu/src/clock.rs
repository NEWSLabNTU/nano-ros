//! Hardware timer clock for ESP32 (QEMU)
//!
//! Uses `esp_hal::time::Instant` for monotonic timestamps.
//! In QEMU with `-icount 3`, the hardware timer works correctly.

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
// FFI exports for zenoh-pico platform layer
// ============================================================================

/// Get current time in milliseconds (called by C platform layer)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    clock_ms()
}

/// Set current time in milliseconds (no-op - hardware timer is authoritative)
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_set_clock_ms(_ms: u64) {
    // No-op: ESP32 uses hardware timer, cannot be set from software
}
