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
    /// Uses raw pointer writes to DEMCR and DWT_CTRL registers since
    /// the platform crate does not take `cortex_m::Peripherals`.
    pub fn enable() {
        unsafe {
            // Set TRCENA in DEMCR (Debug Exception and Monitor Control Register)
            let demcr = 0xE000_EDFC as *mut u32;
            core::ptr::write_volatile(demcr, core::ptr::read_volatile(demcr) | (1 << 24));
            // Set CYCCNTENA in DWT_CTRL
            let dwt_ctrl = 0xE000_1000 as *mut u32;
            core::ptr::write_volatile(dwt_ctrl, core::ptr::read_volatile(dwt_ctrl) | 1);
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
