//! # nros-board-mps2-an385-freertos
//!
//! Board crate for running nros on the MPS2-AN385 (Cortex-M3 + LAN9118)
//! with FreeRTOS + lwIP.
//!
//! Handles hardware init, FreeRTOS kernel startup, lwIP networking, and
//! LAN9118 Ethernet.  Users call [`run()`] with a closure that receives
//! `&Config` and creates an `Executor` for full API access.
//!
//! # Differences from bare-metal (`nros-board-mps2-an385`)
//!
//! - Networking via lwIP sockets (not smoltcp)
//! - FreeRTOS tasks, mutexes, semaphores (real RTOS primitives)
//! - zenoh-pico runs its own background read task
//! - No `zpico-platform-*` or `zpico-smoltcp` crates needed

#![no_std]
// Force-link the zenoh-pico C transport + platform shim when
// `rmw-zenoh` is active. DDS-only builds drop both deps via
// `default-features = false` and reach the linker without the
// zenoh-pico symbol set.
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_platform_shim;
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

mod config;
mod error;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

/// Re-export semihosting for the `println!` macro.
pub use cortex_m_semihosting;

/// Print to QEMU semihosting console.
#[macro_export]
macro_rules! println {
    () => {
        $crate::cortex_m_semihosting::hprintln!()
    };
    ($($arg:tt)*) => {
        $crate::cortex_m_semihosting::hprintln!($($arg)*)
    };
}

/// Exit QEMU with success status.
pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    #[allow(clippy::empty_loop)]
    loop {}
}

/// Exit QEMU with failure status.
pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {}
}
