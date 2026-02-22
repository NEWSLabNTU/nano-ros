//! # nros-mps2-an385
//!
//! Board crate for running nros on the MPS2-AN385 (Cortex-M3 + LAN9118).
//!
//! Handles hardware and network initialization. Users call `run()` with
//! a closure that receives `&Config` and creates an `Executor` for full
//! API access (publishers, subscriptions, services, actions, timers).
//!
//! # Architecture
//!
//! This crate depends on `zpico-platform-mps2-an385` for system primitives
//! (zenoh-pico FFI symbols, clock, memory, RNG) and `zpico-smoltcp` for
//! TCP/IP socket management.

#![no_std]

// Application modules
mod config;
mod error;
mod node;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export semihosting for println! macro
pub use cortex_m_semihosting;

// Re-export zpico-platform for direct access to system primitives
pub use zpico_platform_mps2_an385;

// Re-export main types
pub use config::Config;
pub use node::run;
pub use zpico_platform_mps2_an385::timing::CycleCounter;

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
