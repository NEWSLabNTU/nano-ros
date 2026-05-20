//! Hardware timer clock for ESP32-C3.
//!
//! Uses `esp_hal::time::Instant` for monotonic timestamps.
//! The hardware timer is authoritative — no software counter needed.

use esp_hal::time::Instant;

/// Get the current time in milliseconds.
#[inline]
pub fn clock_ms() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
}

/// Get the current time in microseconds.
#[inline]
pub fn clock_us() -> u64 {
    Instant::now().duration_since_epoch().as_micros()
}
