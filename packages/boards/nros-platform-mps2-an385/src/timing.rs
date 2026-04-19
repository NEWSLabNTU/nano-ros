//! Timing primitives for MPS2-AN385.
//!
//! Provides two types:
//!
//! - [`MonotonicClock`] — portable `Duration`-returning clock, always
//!   available. Use this for general timing, latency measurement, or
//!   fixed-rate loops.
//! - [`CycleCounter`] — cycle-exact `u32` counter backed by the
//!   Cortex-M DWT unit. Only meaningful on platforms that ship a real
//!   DWT counter (MPS2-AN385 and STM32F4); not every platform provides
//!   one. Use for WCET measurement where sub-microsecond precision
//!   matters.
//!
//! # QEMU Note
//!
//! QEMU does not fully emulate the DWT cycle counter on all machines.
//! `CycleCounter::read()` may return 0 on QEMU — this is expected. The
//! API is validated on real hardware (STM32F4) where DWT is
//! hardware-backed. `MonotonicClock` works on both QEMU and hardware.

use core::time::Duration;

use super::clock;

// ============================================================================
// MonotonicClock (portable, always available)
// ============================================================================

/// Portable monotonic clock returning `core::time::Duration`.
///
/// Backed by the SysTick-driven `clock_ms` counter. Resolution is 1 ms
/// on MPS2-AN385 (the platform's clock source is millisecond-resolution).
pub struct MonotonicClock;

impl MonotonicClock {
    /// Returns time elapsed since an unspecified epoch (monotonic).
    pub fn now() -> Duration {
        Duration::from_millis(clock::clock_ms())
    }

    /// Measure the elapsed time of a closure.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, Duration) {
        let start = Self::now();
        let result = f();
        let elapsed = Self::now().saturating_sub(start);
        (result, elapsed)
    }
}

// ============================================================================
// CycleCounter (DWT-backed, cycle-exact)
// ============================================================================

/// Cycle-exact measurement using the Cortex-M DWT cycle counter.
///
/// The DWT unit provides a 32-bit cycle counter that increments at the
/// CPU clock rate. Prefer [`MonotonicClock`] for portable timing;
/// reach for `CycleCounter` only when you need cycle-exact precision
/// (WCET analysis, low-level benchmarks).
pub struct CycleCounter;

impl CycleCounter {
    /// Enable the DWT cycle counter (call once at startup).
    pub fn enable() {
        use cortex_m::peripheral::{DCB, DWT};

        unsafe {
            (*DCB::PTR).demcr.modify(|w| w | (1 << 24));
            (*DWT::PTR).ctrl.modify(|w| w | 1);
        }
    }

    /// Read the current DWT cycle count.
    pub fn read() -> u32 {
        cortex_m::peripheral::DWT::cycle_count()
    }

    /// Measure the cycle count of a closure.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, u32) {
        let start = Self::read();
        let result = f();
        let elapsed = Self::read().wrapping_sub(start);
        (result, elapsed)
    }
}
