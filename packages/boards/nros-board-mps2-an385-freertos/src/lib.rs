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
// Phase 121.3 — the canonical `nros_platform_*` symbols come from the
// FreeRTOS C port (`packages/core/nros-platform-freertos/src/*.c`)
// compiled in-tree by build.rs. The Rust kernel crate that previously
// emitted the same symbols via the `nros_platform_export!` macro was
// deleted in Phase 121.3.deprecate-rust-remove.
//
// Keep `nros-platform` linked even for DDS-only examples that do not
// reference it directly; its `global-allocator` feature installs the
// Rust allocator adapter over the FreeRTOS C heap.
extern crate nros_platform as _;

// Force-link the zenoh-pico C transport + platform shim when
// `rmw-zenoh` is active. DDS-only builds drop both deps via
// `default-features = false` and reach the linker without the
// zenoh-pico symbol set.
#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

// Phase 152.1.B.5 — `Config` + the FreeRTOS-task plumbing live in
// the generic `nros-board-freertos` crate. The overlay only
// implements the three `BoardInit` / `BoardPrint` / `BoardExit`
// traits + provides a thin non-generic `run()` wrapper.
use nros_board_freertos::{BoardExit, BoardInit, BoardPrint};
pub use nros_board_freertos::Config;

/// Per-board marker for trait dispatch into
/// `nros_board_freertos::run::<Mps2An385, _, _>`.
pub struct Mps2An385;

impl BoardInit for Mps2An385 {
    type Config = Config;

    fn init_hardware(_cfg: &Config) {
        // FreeRTOS network init requires the scheduler to be
        // running (tcpip_init creates tcpip_thread). All
        // meaningful init happens inside the app task created
        // by the generic `run`.
    }
}

impl BoardPrint for Mps2An385 {
    fn println(args: core::fmt::Arguments<'_>) {
        // `hprintln!` only takes a format string + args, not a
        // pre-built `Arguments`. Use `hio::hstdout` + `writeln!`
        // so we can forward `Arguments` straight through.
        use core::fmt::Write;
        if let Ok(mut stdout) = cortex_m_semihosting::hio::hstdout() {
            let _ = writeln!(stdout, "{}", args);
        }
    }
}

impl BoardExit for Mps2An385 {
    fn exit_success() -> ! {
        exit_success()
    }

    fn exit_failure() -> ! {
        exit_failure()
    }
}

/// Initialise pre-scheduler hardware. Delegates through the
/// trait so the overlay's `init_hardware` is reachable both
/// from `run()` (via the generic `nros_board_freertos::run<B>`)
/// and standalone (e.g. board-side bring-up tests).
pub fn init_hardware(cfg: &Config) {
    <Mps2An385 as BoardInit>::init_hardware(cfg);
}

/// Run an application on QEMU MPS2-AN385 with FreeRTOS + lwIP.
///
/// Thin wrapper over `nros_board_freertos::run::<Mps2An385, _, _>`
/// so users do not have to spell the trait turbofish themselves.
///
/// # Example
///
/// ```ignore
/// use nros_board_mps2_an385_freertos::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     // ...
///     Ok::<(), NodeError>(())
/// })
/// ```
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    nros_board_freertos::run::<Mps2An385, F, E>(config, f)
}

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
