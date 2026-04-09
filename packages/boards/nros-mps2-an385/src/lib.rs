//! # nros-mps2-an385
//!
//! Board crate for running nros on the MPS2-AN385 (Cortex-M3 + LAN9118).
//!
//! Handles hardware and transport initialization. Users call `run()` with
//! a closure that receives `&Config` and creates an `Executor` for full
//! API access (publishers, subscriptions, services, actions, timers).
//!
//! # Transport Features
//!
//! - `ethernet` (default) — LAN9118 + smoltcp TCP/IP stack
//! - `serial` — CMSDK UART via zpico-serial
//!
//! At least one transport must be enabled.
//!
//! # Architecture
//!
//! This crate depends on `nros-platform-mps2-an385` for system primitives
//! (clock, memory, RNG) and `zpico-platform-shim` for zenoh-pico FFI
//! symbols. Transport layer is provided by `zpico-smoltcp` (Ethernet)
//! or `zpico-serial` (serial).

#![no_std]
extern crate zpico_platform_shim;

// Application modules
mod config;
mod error;
#[cfg(feature = "ethernet")]
pub mod network;
mod node;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export semihosting for println! macro
pub use cortex_m_semihosting;

// Re-export platform crate for direct access to system primitives
pub use nros_platform_mps2_an385;

// Re-export main types
pub use config::Config;
pub use node::{init_hardware, run};
pub use nros_platform_mps2_an385::timing::CycleCounter;

/// Print to QEMU semihosting console
#[macro_export]
macro_rules! println {
    () => {
        $crate::cortex_m_semihosting::hprintln!()
    };
    ($($arg:tt)*) => {
        $crate::cortex_m_semihosting::hprintln!($($arg)*)
    };
}

/// Exit QEMU with success status
pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}

/// Exit QEMU with failure status
pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}
