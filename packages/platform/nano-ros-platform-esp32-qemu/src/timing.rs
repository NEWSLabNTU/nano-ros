//! Timing measurement using the ESP32-C3 hardware timer.
//!
//! ESP32-C3 is RISC-V — it has no ARM DWT. This module uses
//! `esp_hal::time::Instant` for nanosecond-resolution timing,
//! returning elapsed nanoseconds as a u32 cycle-count proxy.
//!
//! # Example
//!
//! ```ignore
//! use nano_ros_platform_esp32_qemu::CycleCounter;
//!
//! let (result, nanos) = CycleCounter::measure(|| {
//!     // operation to benchmark
//! });
//! esp_println::println!("Took {} ns", nanos);
//! ```

use esp_hal::time::Instant;

/// Timing measurement using the hardware timer.
///
/// Values returned by `read()` and `measure()` are in microseconds.
pub struct CycleCounter;

impl CycleCounter {
    /// No-op: the ESP32-C3 hardware timer always runs.
    pub fn enable() {}

    /// Read the current timestamp (low 32 bits of microseconds).
    pub fn read() -> u32 {
        Instant::now().duration_since_epoch().as_micros() as u32
    }

    /// Measure the elapsed microseconds of a closure.
    ///
    /// Returns `(result, elapsed_micros)`.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, u32) {
        let start = Self::read();
        let result = f();
        let elapsed = Self::read().wrapping_sub(start);
        (result, elapsed)
    }
}
