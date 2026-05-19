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
    register_log_writer();
    nros_board_threadx::run::<ThreadxLinux, Config, F, E>(config, f)
}

/// Phase 88 — register a stdout writer with `nros-platform-threadx`'s
/// log fn-ptr slot. ThreadX-Linux runs ThreadX kernel as a POSIX
/// process, so stderr is the natural sink. Called once from `run()`
/// before any thread spawns.
fn register_log_writer() {
    unsafe extern "C" fn writer(
        severity: u8,
        name_ptr: *const u8,
        name_len: usize,
        msg_ptr: *const u8,
        msg_len: usize,
    ) {
        let label_bytes: &[u8] = match severity {
            0 => b"[TRACE] ",
            1 => b"[DEBUG] ",
            2 => b"[INFO] ",
            3 => b"[WARN] ",
            4 => b"[ERROR] ",
            5 => b"[FATAL] ",
            _ => b"[?] ",
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
        unsafe extern "C" {
            fn write(fd: i32, buf: *const u8, n: usize) -> isize;
        }
        const STDERR_FD: i32 = 2;
        // SAFETY: stderr is always open on POSIX; each write is a
        // single syscall — partial writes are tolerated.
        unsafe {
            write(STDERR_FD, label_bytes.as_ptr(), label_bytes.len());
            if !name.is_empty() {
                write(STDERR_FD, name.as_ptr(), name.len());
                write(STDERR_FD, b": ".as_ptr(), 2);
            }
            write(STDERR_FD, msg.as_ptr(), msg.len());
            write(STDERR_FD, b"\n".as_ptr(), 1);
        }
    }
    // SAFETY: extern decl matches `<nros/platform.h>`; the writer
    // honours the documented contract.
    unsafe {
        nros_platform_cffi::nros_platform_register_log_writer(Some(writer), None);
    }
}
