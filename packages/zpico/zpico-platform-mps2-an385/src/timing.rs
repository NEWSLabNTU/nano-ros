//! Cycle-accurate timing measurement using the ARM DWT cycle counter.
//!
//! The Cortex-M DWT (Data Watchpoint and Trace) unit provides a 32-bit
//! cycle counter that increments at the CPU clock rate.
//!
//! # QEMU Note
//!
//! QEMU does not fully emulate the DWT cycle counter on all machines.
//! Cycle counts may read as 0 on QEMU — this is expected. The API is
//! validated on real hardware (STM32F4) where DWT is hardware-backed.

/// Cycle-accurate measurement using the DWT cycle counter.
pub struct CycleCounter;

impl CycleCounter {
    /// Enable the DWT cycle counter (call once at startup).
    ///
    /// Accesses DCB and DWT register blocks via the cortex-m typed
    /// interface. Requires unsafe because the platform crate cannot
    /// take `cortex_m::Peripherals` (singleton owned by the board
    /// crate / application).
    pub fn enable() {
        use cortex_m::peripheral::{DCB, DWT};

        // SAFETY: Called once at startup before any concurrent DCB/DWT access.
        // Platform crate cannot take Peripherals (singleton), so we access
        // the register blocks directly through cortex-m typed pointers.
        unsafe {
            // Set TRCENA in DEMCR (enables the DWT unit)
            (*DCB::PTR).demcr.modify(|w| w | (1 << 24));
            // Set CYCCNTENA in DWT_CTRL (enables the cycle counter)
            (*DWT::PTR).ctrl.modify(|w| w | 1);
        }
    }

    /// Read the current DWT cycle count.
    pub fn read() -> u32 {
        cortex_m::peripheral::DWT::cycle_count()
    }

    /// Measure the cycle count of a closure.
    ///
    /// Returns `(result, elapsed_cycles)`.
    pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, u32) {
        let start = Self::read();
        let result = f();
        let elapsed = Self::read().wrapping_sub(start);
        (result, elapsed)
    }
}
