//! Microsecond-resolution timing for ESP32-C3.
//!
//! Uses `esp_hal::time::Instant` for measurement.

use esp_hal::time::Instant;

/// Timing measurement using ESP32 hardware timer.
pub struct CycleCounter;

impl CycleCounter {
    /// Enable the timer (no-op on ESP32 — hardware timer always runs).
    pub fn enable() {}

    /// Read the current time in microseconds.
    pub fn read() -> u32 {
        Instant::now().duration_since_epoch().as_micros() as u32
    }

    /// Measure the elapsed microseconds of a closure.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, u32) {
        let start = Self::read();
        let result = f();
        let elapsed = Self::read().wrapping_sub(start);
        (result, elapsed)
    }
}
