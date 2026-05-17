//! Phase 152.2.B.4 — thin non-generic `run` + `init_hardware`
//! wrappers over the generic `nros_board_threadx::run<B>` lift.

use crate::config::Config;
use crate::ThreadxQemuRiscv64;

/// Initialize pre-kernel hardware for ThreadX QEMU RISC-V.
///
/// No-op today — full NetX-Duo + virtio-net bring-up happens in
/// `tx_application_define()` (via `nros_board_init_eth`) after
/// the kernel starts.
pub fn init_hardware(_config: &Config) {}

/// Run an application on QEMU RISC-V with ThreadX + NetX Duo + virtio-net.
///
/// Thin wrapper over `nros_board_threadx::run::<ThreadxQemuRiscv64, _, _, _>`.
///
/// # Example
///
/// ```ignore
/// use nros_board_threadx_qemu_riscv64::{Config, run};
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
    nros_board_threadx::run::<ThreadxQemuRiscv64, Config, F, E>(config, f)
}
