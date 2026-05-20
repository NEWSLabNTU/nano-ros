//! Phase 152.2.B.4 — thin non-generic `run` + `init_hardware`
//! wrappers over the generic `nros_board_threadx::run<B>` lift.

use crate::{ThreadxQemuRiscv64, config::Config};

/// Initialize pre-kernel hardware for ThreadX QEMU RISC-V.
///
/// No-op today — full NetX-Duo + virtio-net bring-up happens in
/// `tx_application_define()` (via `nros_board_init_eth`) after
/// the kernel starts.
pub fn init_hardware(_config: &Config) {}

/// Run an application on QEMU RISC-V with ThreadX + NetX Duo + virtio-net.
///
/// Thin wrapper over `nros_board_threadx::run::<ThreadxQemuRiscv64, _, _>`.
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
    register_log_writer();
    nros_board_threadx::run::<ThreadxQemuRiscv64, F, E>(config, f)
}

/// Phase 88 — register a UART writer with `nros-platform-threadx`'s
/// log fn-ptr slot. Called once from `run()` before any thread spawns.
fn register_log_writer() {
    use core::fmt::Write as _;
    unsafe extern "C" fn writer(
        severity: u8,
        name_ptr: *const u8,
        name_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    ) {
        let label = match severity {
            0 => "TRACE",
            1 => "DEBUG",
            2 => "INFO",
            3 => "WARN",
            4 => "ERROR",
            5 => "FATAL",
            _ => "?",
        };
        // SAFETY: caller passes valid `&[u8]` slices that outlive
        // the call; empty-name case collapses to an empty slice.
        let name: &[u8] = if name_ptr.is_null() || name_len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(name_ptr, name_len) }
        };
        let msg: &[u8] = if msg_ptr.is_null() || msg_len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(msg_ptr, msg_len) }
        };
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(msg).unwrap_or("");
        let mut buf = super::UartWriter;
        if !name_str.is_empty() {
            let _ = writeln!(buf, "[{}] {}: {}", label, name_str, msg_str);
        } else {
            let _ = writeln!(buf, "[{}] {}", label, msg_str);
        }
    }
    // SAFETY: extern decl matches `<nros/platform.h>`; the writer
    // honours the documented contract (slice validity, no panics).
    unsafe {
        nros_platform_cffi::nros_platform_register_log_writer(Some(writer), None);
    }
}
