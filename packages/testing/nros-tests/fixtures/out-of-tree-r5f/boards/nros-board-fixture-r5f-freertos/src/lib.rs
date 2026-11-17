//! Board crate for the out-of-tree board package `fixture-r5f-freertos`
//! (a test fixture, not a real board). The C side (startup, tick hooks over the
//! vendor SDK, weak netif hooks) is `c/board_fixture_r5f.c`; the FreeRTOS
//! family glue comes from `nros-board-freertos`.

#![no_std]

// Board crates link the platform even when no Rust symbol is referenced.
extern crate nros_platform as _;

#[cfg(feature = "rmw-zenoh")]
extern crate zpico_sys;

pub use nros_board_freertos::{BaseConfig, Config};

/// The board's marker type, named in this crate's `[package.metadata.nros.board] type`.
pub struct FixtureR5f;

unsafe extern "C" {
    /// The vendor SDK's debug UART (`fixture_r5f_sdk.h`).
    fn fxsdk_uart_putc(c: core::ffi::c_char);
}

struct Uart;

impl core::fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            // SAFETY: the SDK's UART write is a polled register store.
            unsafe { fxsdk_uart_putc(b as core::ffi::c_char) };
        }
        Ok(())
    }
}

impl nros_platform::BoardInit for FixtureR5f {
    fn init_hardware() {
        // The UART is brought up by the C startup before `main`; the network
        // needs the scheduler, so it starts in the family driver's app task.
    }
}

impl nros_platform::BoardPrint for FixtureR5f {
    fn println(args: core::fmt::Arguments<'_>) {
        use core::fmt::Write;
        let _ = writeln!(Uart, "{args}");
    }
}

impl nros_platform::BoardExit for FixtureR5f {
    fn exit_success() -> ! {
        loop {
            core::hint::spin_loop();
        }
    }

    fn exit_failure() -> ! {
        loop {
            core::hint::spin_loop();
        }
    }
}

impl nros_platform::BoardEntry for FixtureR5f {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        nros_board_freertos::run_entry::<FixtureR5f, F, E>(Config::default(), None, setup)
    }

    fn run_with_deploy<F, E>(deploy: &nros_platform::DeployOverlay, setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut nros_platform::RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let mut config = Config::default();
        config.base.apply_overlay(deploy);
        nros_board_freertos::run_entry::<FixtureR5f, F, E>(config, deploy.boot_config, setup)
    }
}
