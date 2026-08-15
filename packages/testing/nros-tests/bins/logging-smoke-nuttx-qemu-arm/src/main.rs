//! Phase 88.15.c — NuttX QEMU ARM nros-log smoke fixture.
//!
//! Boots NuttX through the board crate's `nsh_main` override
//! (`nros-board-nuttx-qemu/src/entry.rs`: NuttX init → `nsh_main` →
//! `nsh_initialize()` → Rust `main`), then drives every severity through
//! `nros-log`. The NuttX C platform path routes records through syslog so
//! the QEMU harness can assert the captured UART output.
//!
//! #127 — this bin previously carried its OWN `nsh_main` + a build.rs copy
//! of the NuttX image link. Both are gone: the board dep supplies the init
//! entrypoint and (via `nros_board_common::nuttx_image_link`) the
//! propagating link directives; `.cargo/config.toml` carries the static
//! link args.

// phase-359 W7 — the NuttX family is `no_std`; the board crate supplies this
// image's `#[panic_handler]` and `#[global_allocator]`, and `nros::main!` is not
// used here, so this bin spells out the `extern "C" fn main` that `nsh_main`
// calls (libstd's `lang_start` used to supply that symbol).
#![no_std]
#![no_main]

// Link-anchor the board crate: its `entry.rs` `nsh_main` (the NuttX
// `CONFIG_INIT_ENTRYPOINT`) and its build.rs's propagating image-link
// directives are the whole point of the dependency.
use nros_board_nuttx_qemu as _;
use nros_log::{
    Logger, Severity, init, nros_debug, nros_error, nros_fatal, nros_info, nros_trace, nros_warn,
    register_logger, sinks,
};

static LOGGER: Logger = Logger::new("smoke");

/// The image entry NuttX's `nsh_main` calls; signature matches the
/// `extern "C" fn main(argc, argv)` that the board's `entry.rs` declares.
#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: i32, _argv: *const *const core::ffi::c_char) -> i32 {
    register_logger(&LOGGER);
    init(sinks::default());
    LOGGER.set_level(Severity::Trace);

    nros_trace!(&LOGGER, "trace payload");
    nros_debug!(&LOGGER, "debug payload");
    nros_info!(&LOGGER, "info payload");
    nros_warn!(&LOGGER, "warn payload");
    nros_error!(&LOGGER, "error payload");
    nros_fatal!(&LOGGER, "fatal payload");
    nros_log::flush();
    0
}
