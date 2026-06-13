//! # nros-board-mps2-an385
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
//! symbols. Transport layer is provided by `nros-smoltcp` (Ethernet)
//! or `zpico-serial` (serial).

#![no_std]

// `smoltcp_clock_now_ms` (referenced by `nros-smoltcp::bridge`) is
// provided by `zpico-sys`'s `platform_aliases.c`, which forwards to
// `nros_platform_time_now_ms` — the canonical platform C ABI.
// Phase 129 retired the per-board override.

// Application modules
mod config;
mod error;
// Phase 244.D1 enabler — `nros_platform::BoardEntry` for `nros::main!()`.
// Behind a feature so the legacy `run(Config, closure)` consumers don't pull
// the `nros` umbrella + executor stack.
#[cfg(feature = "board-entry")]
mod entry;
#[cfg(feature = "ethernet")]
pub mod network;
mod node;
// Phase 207.2 — XRCE custom-transport callbacks bound to the CMSDK UART0.
// Off by default; opt in via `features = ["xrce-transport"]` from an XRCE
// example (forwards through to `serial`).
#[cfg(feature = "xrce-transport")]
pub mod xrce_transport;

// Phase 248 C5a (#60 T4) — the board owns XRCE backend linking. Force-link the
// `nros-rmw-xrce-cffi` backend rlib so its `RMW_INIT_ENTRIES` self-register
// section survives stable-Rust rlib pruning and reaches the binary, WITHOUT a
// consumer naming the backend. Mirrors `__FORCE_LINK_XRCE` in `nros/src/lib.rs`
// (referencing `register` keeps both the symbol and its linker section alive).
// On bare-metal (`target_os = "none"`) the section walker is a no-op, so the XRCE
// example still drives the explicit registration; this guarantees the rlib +
// custom-transport ops are not pruned first. Inert unless `xrce-transport` is on.
#[cfg(feature = "xrce-transport")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_XRCE: fn() -> Result<(), nros_rmw_xrce_cffi::RegisterError> =
    nros_rmw_xrce_cffi::register;

// Re-export entry macro
pub use cortex_m_rt::entry;

// Re-export semihosting for println! macro
pub use cortex_m_semihosting;

// Re-export platform crate for direct access to system primitives
pub use nros_platform_mps2_an385;

// Re-export main types
pub use config::Config;
pub use node::{Mps2An385, init_hardware, run};
pub use nros_platform::BoardConfig;
pub use nros_platform_mps2_an385::timing::{CycleCounter, MonotonicClock};

// Phase 127.D — re-export nros-smoltcp so RTIC examples can read the
// poll/RX diagnostic counters without adding a direct dep.
#[cfg(feature = "ethernet")]
pub use lan9118_smoltcp;
#[cfg(feature = "ethernet")]
pub use nros_smoltcp;

/// Phase 127.D — install `wfi` as the busy-wait idle hook so
/// `Executor::open`'s connect/handshake polls release the CPU to
/// QEMU's main loop between iterations. Must be called AFTER an IRQ
/// source is armed; for RTIC examples that means immediately after
/// `Mono::start(cx.core.SYST, ..)`. See
/// `nros_platform_mps2_an385::sleep::enable_wfi_idle` for the safety
/// contract and rationale.
///
/// Installs the hook on BOTH busy-wait sites:
/// - `nros_baremetal_common::sleep::sleep_ms` — used by `z_sleep_ms`.
/// - `nros_smoltcp::do_poll` — used by every `<PlatformTcp>::open`/
///   `send`/`read` loop iteration. Without this second hook, the
///   `Executor::open` connect loop would spin without yielding.
#[cfg(feature = "ethernet")]
pub fn enable_wfi_idle() {
    nros_platform_mps2_an385::sleep::enable_wfi_idle();
    nros_smoltcp::set_idle_callback(nros_platform_mps2_an385::sleep::nros_mps2_an385_wfi_idle);
}

/// Serial-only build variant of [`enable_wfi_idle`] — only the
/// busy-wait sleep loop needs the hook; there is no smoltcp bridge.
#[cfg(all(feature = "serial", not(feature = "ethernet")))]
pub fn enable_wfi_idle() {
    nros_platform_mps2_an385::sleep::enable_wfi_idle();
}

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
