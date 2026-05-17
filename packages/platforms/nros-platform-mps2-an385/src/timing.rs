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

    /// Convert a DWT cycle count to nanoseconds at the supplied
    /// `SystemCoreClock` rate.
    ///
    /// Uses `u64` arithmetic to avoid overflow at typical Cortex-M
    /// clock rates (25 MHz → 50 ns/cycle * 2³² cycles fits in u64).
    /// Phase 141.B.1: lets the wake-latency probe convert its
    /// `clock_cycles` deltas into a transport-comparable µs/ns
    /// distribution without dragging in `core::time::Duration` math.
    pub const fn cycles_to_ns(cycles: u32, system_core_clock_hz: u32) -> u64 {
        (cycles as u64 * 1_000_000_000) / system_core_clock_hz as u64
    }
}

/// Free-function alias for `CycleCounter::read()` — Phase 141.B.1
/// names the probe entry-points `clock_cycles()` / `cycles_to_ns(…)`
/// at the module level so wake-latency probe call sites stay
/// terse and don't need to import the `CycleCounter` type.
#[inline]
pub fn clock_cycles() -> u32 {
    CycleCounter::read()
}

/// Free-function alias for [`CycleCounter::cycles_to_ns`].
#[inline]
pub const fn cycles_to_ns(cycles: u32, system_core_clock_hz: u32) -> u64 {
    CycleCounter::cycles_to_ns(cycles, system_core_clock_hz)
}
