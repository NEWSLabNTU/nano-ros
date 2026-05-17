//! Phase 152.2.B.4 — thin non-generic `run` + `init_hardware`
//! wrappers over the generic `nros_board_threadx::run<B>` lift.

use crate::config::Config;
use crate::ThreadxLinux;

/// Initialize pre-kernel hardware for ThreadX Linux simulation.
///
/// No-op today — ThreadX network init (NSOS shim) happens inside
/// `tx_application_define()` in C code, after the kernel starts.
pub fn init_hardware(_config: &Config) {}

/// Run an application on Linux with ThreadX + NSOS.
///
/// Thin wrapper over `nros_board_threadx::run::<ThreadxLinux, _, _, _>`
/// so users do not have to spell the trait turbofish.
///
/// # Example
///
/// ```ignore
/// use nros_board_threadx_linux::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     Ok::<(), NodeError>(())
/// })
/// ```
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    nros_board_threadx::run::<ThreadxLinux, Config, F, E>(config, f)
}
