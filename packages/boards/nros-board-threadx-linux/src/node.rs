//! Phase 152.2.B.4 — thin non-generic `run` + `init_hardware`
//! wrappers over the generic `nros_board_threadx::run<B>` lift.

use crate::config::Config;

/// Initialize pre-kernel hardware for ThreadX Linux simulation.
///
/// No-op today — ThreadX network init (NSOS shim) happens inside
/// `tx_application_define()` in C code, after the kernel starts.
pub fn init_hardware(_config: &Config) {}

// Phase 313 W-threadx (#0243) — the legacy free `run(Config, closure)` (a thin
// wrapper over the legacy `nros_board_threadx::run<B>` family lift) is RETIRED.
// The live entries are `<ThreadxLinux as nros_platform::board::BoardEntry>::run`
// (full app, used by `nros::main!`) and `ThreadxLinux::run_bare` (no-session, for
// logging/init-only fixtures) — both in lib.rs.

/// Phase 212.N.3 — crate-internal accessor for the log-writer
/// registration so the new `nros_platform::BoardEntry::run` impl can
/// seed the platform log slot before kernel entry (same shape the
/// legacy `run` wrapper already uses).
pub(crate) fn register_log_writer_public() {
    register_log_writer();
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
        let mut line = [0u8; 512];
        let mut used = 0usize;
        fn append(dst: &mut [u8], used: &mut usize, src: &[u8]) {
            let remaining = dst.len().saturating_sub(*used);
            let n = src.len().min(remaining);
            dst[*used..*used + n].copy_from_slice(&src[..n]);
            *used += n;
        }
        append(&mut line, &mut used, label_bytes);
        if !name.is_empty() {
            append(&mut line, &mut used, name);
            append(&mut line, &mut used, b": ");
        }
        append(&mut line, &mut used, msg);
        append(&mut line, &mut used, b"\n");

        unsafe extern "C" {
            fn syscall(num: isize, ...) -> isize;
        }
        const STDERR_FD: i32 = 2;
        const SYS_WRITE: isize = 1;
        // SAFETY: stderr is always open on Linux/POSIX; use the
        // syscall path directly because the ThreadX Linux port
        // provides a weak `write` symbol that does not write host fds.
        unsafe {
            syscall(SYS_WRITE, STDERR_FD, line.as_ptr(), used);
        }
    }
    // SAFETY: extern decl matches `<nros/platform.h>`; the writer
    // honours the documented contract.
    unsafe {
        nros_platform_cffi::nros_platform_register_log_writer(Some(writer), None);
    }
}
