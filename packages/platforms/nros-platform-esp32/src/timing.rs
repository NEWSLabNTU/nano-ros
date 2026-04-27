//! Timing primitives for ESP32.
//!
//! ESP32 does not provide a standard cycle counter (the 32-bit value
//! returned by the old `CycleCounter` was actually microseconds from
//! `esp_hal::time::Instant`, which is a monotonic clock rather than a
//! cycle counter). Provides a [`MonotonicClock`] returning
//! `core::time::Duration` at microsecond resolution — the portable
//! replacement for the fake `CycleCounter`.

use core::time::Duration;

use esp_hal::time::Instant;

/// Portable monotonic clock returning `core::time::Duration`.
///
/// Backed by `esp_hal::time::Instant`; resolution is 1 µs.
pub struct MonotonicClock;

impl MonotonicClock {
    /// Returns time elapsed since boot (monotonic).
    pub fn now() -> Duration {
        Duration::from_micros(Instant::now().duration_since_epoch().as_micros())
    }

    /// Measure the elapsed time of a closure.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, Duration) {
        let start = Self::now();
        let result = f();
        let elapsed = Self::now().saturating_sub(start);
        (result, elapsed)
    }
}
