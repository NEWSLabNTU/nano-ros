//! Phase 212.N.3 — `nros-platform::Board*` impls for ESP32-C3.
//!
//! Wires the existing `nros-board-esp32` crate onto the
//! `nros-board-bare-metal` direct-exec family driver via a single
//! `BoardEntry::run` one-liner. The legacy `run()`/`init_hardware()`
//! free functions (in `node.rs`) stay untouched — they keep working
//! for Node pkgs that haven't migrated to the 212.N codegen yet.
//!
//! ## Surface
//!
//! - [`Esp32C3`] — ZST that implements the four 212.N.1 board traits
//!   plus the [`DirectExec`] marker so [`run_entry`] can drive boot.
//! - `BoardInit` calls [`nros_platform_esp32::sleep::init_clock`]; the
//!   transport / WiFi bringup is left to the legacy `node::run` path
//!   (codegen will pick that up in 212.N.4 / N.5).
//! - `BoardExit` diverges with a `wfi`-style `spin_loop` since ESP32
//!   has no host-side exit — same shape as the legacy `node::run`
//!   tail.
//!
//! [`DirectExec`]: nros_board_bare_metal::DirectExec
//! [`run_entry`]: nros_board_bare_metal::run_entry

use nros_board_bare_metal::{DirectExec, run_entry};
use nros_platform::{BoardEntry, BoardExit, BoardInit, BoardPrint, RuntimeCtx};

/// ESP32-C3 board zero-sized type (the tier-1 entry per 212.N.3).
///
/// One impl per board — the type itself carries no state; per-board
/// statics (heap, WiFi handles, smoltcp interface) live in
/// [`crate::node`].
pub struct Esp32C3;

impl BoardInit for Esp32C3 {
    fn init_hardware() {
        // Minimal hardware init for the 212.N.3 surface: install the
        // monotonic-clock pointer that the platform sleep loop reads.
        // The legacy `node::init_hardware` (which also brings up the
        // ESP-HAL peripherals + heap + transport) keeps running from
        // the existing `node::run` path; the 212.N codegen will own
        // the transport bringup once N.4 / N.5 land.
        nros_platform_esp32::sleep::init_clock();
    }
}

impl BoardPrint for Esp32C3 {
    fn println(args: core::fmt::Arguments<'_>) {
        // `esp_println::println!` is the canonical stdout writer on
        // ESP32-C3 (it bridges to the JTAG-USB / UART console). Pass
        // `core::fmt::Arguments` through with a single `{}` so the
        // macro doesn't re-format a literal.
        esp_println::println!("{}", args);
    }
}

impl BoardExit for Esp32C3 {
    fn exit_success() -> ! {
        // ESP32-C3 has no host-side exit — same tail as the legacy
        // `node::run` `Ok` branch. Spin-loop diverges so the trait's
        // `-> !` is satisfied.
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }

    fn exit_failure() -> ! {
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }
}

impl DirectExec for Esp32C3 {}

impl BoardEntry for Esp32C3 {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        run_entry::<Self, F, E>(setup)
    }
}
