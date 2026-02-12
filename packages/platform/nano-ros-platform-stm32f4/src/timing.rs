//! Cycle-accurate timing measurement using the ARM DWT cycle counter.
//!
//! The Cortex-M4 DWT (Data Watchpoint and Trace) unit provides a 32-bit
//! cycle counter that increments at the CPU clock rate (168 MHz on
//! Nucleo-F429ZI).
//!
//! The DWT is already enabled by `run_node()`, so `enable()` is a
//! defensive no-op re-enable.
//!
//! # Example
//!
//! ```ignore
//! use nano_ros_platform_stm32f4::CycleCounter;
//!
//! let (result, cycles) = CycleCounter::measure(|| {
//!     // operation to benchmark
//! });
//! defmt::info!("Took {} cycles", cycles);
//! ```

/// Cycle-accurate measurement using the DWT cycle counter.
pub struct CycleCounter;

impl CycleCounter {
    /// Enable the DWT cycle counter.
    ///
    /// On STM32F4, the DWT is already enabled by `run_node()`.
    /// This is a defensive re-enable using raw register writes.
    pub fn enable() {
        unsafe {
            let demcr = 0xE000_EDFC as *mut u32;
            core::ptr::write_volatile(demcr, core::ptr::read_volatile(demcr) | (1 << 24));
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
