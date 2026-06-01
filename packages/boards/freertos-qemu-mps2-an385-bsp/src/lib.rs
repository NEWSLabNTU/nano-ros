//! Phase 212.H.3 — FreeRTOS BSP crate (cargo-native adapter).
//!
//! Re-exports `nros-board-mps2-an385-freertos` and layers the Phase
//! 212 system-codegen shape on top. The heavyweight FreeRTOS-Kernel +
//! lwIP + LAN9118 compile is owned by the underlying board crate; this
//! crate's `build.rs` only adds `nros_config_generated.h` +
//! `system_main.c` (per the
//! `docs/design/rtos-integration-pattern.md` §3 contract).
//!
//! # Usage
//!
//! ```ignore
//! // firmware/src/main.rs — 5 lines
//! #![no_std]
//! #![no_main]
//! use panic_semihosting as _;
//! #[unsafe(no_mangle)]
//! extern "C" fn _start() -> ! { freertos_qemu_mps2_an385_bsp::nros_run() }
//! ```

#![no_std]

// Force-link the underlying board crate so its `cargo:rustc-link-lib`
// directives + linker scripts reach the final firmware. Without the
// `extern crate _` reference, cargo would drop the rlib at link time
// for `staticlib`-less consumers.
extern crate nros_board_mps2_an385_freertos as board;

pub use nros_board_mps2_an385_freertos::{
    Config, Mps2An385, exit_failure, exit_success, init_hardware, println,
};

// `system_main` is C-linkage and provided by the build-script-generated
// `system_main.c`. Stub it weakly here so a consumer that only wants the
// adapter shape (no real components) still links.
unsafe extern "C" {
    fn nros_system_main();
}

/// Phase 212.H.3 entry point.
///
/// Calls the build-script-generated `nros_system_main()` to register
/// every `[[component]]` listed in the bringup `system.toml`, then
/// hands control to the underlying board crate's
/// `nros_board_mps2_an385_freertos::run()` which sets up FreeRTOS +
/// lwIP and launches `vTaskStartScheduler`.
///
/// Marked `-> !` because the FreeRTOS scheduler never returns.
pub fn nros_run() -> ! {
    // SAFETY: `nros_system_main` has C linkage and no arguments; it
    // walks the baked component table populated at build time.
    unsafe { nros_system_main() };
    let cfg = Config::default();
    board::run::<_, &'static str>(cfg, |_cfg: &Config| -> Result<(), &'static str> { Ok(()) })
}
