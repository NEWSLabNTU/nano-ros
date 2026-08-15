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

        // The write goes through the board's C glue (issue 0585). Two reasons,
        // and the second is why this is not done in Rust:
        //
        //  * A plain `write()` is captured by the WEAK `write` the ThreadX
        //    Linux port defines, which never reaches host fds. The glue issues
        //    the raw syscall instead.
        //  * The syscall NUMBER is per-ARCHITECTURE, not per-OS — 1 on x86_64,
        //    64 on every asm-generic port (aarch64, riscv64, …), where 1 is
        //    `io_destroy`. This site used to hardcode the x86 value and then a
        //    hand-written per-arch table. `<sys/syscall.h>` already knows the
        //    answer for whatever host is compiling, so the C side asks it and
        //    there is no table to keep correct.
        //
        // Getting this wrong does not fail — it goes SILENT (the fixture
        // booted, flushed, exited 0 and printed nothing), which is exactly why
        // the number must come from the headers rather than from us.
        unsafe extern "C" {
            fn nros_board_log_write_stderr(buf: *const u8, len: usize);
        }
        // SAFETY: `line[..used]` is initialised and outlives the call; the glue
        // treats it as a byte count, not a C string, and tolerates len == 0.
        unsafe {
            nros_board_log_write_stderr(line.as_ptr(), used);
        }
    }
    // SAFETY: extern decl matches `<nros/platform.h>`; the writer
    // honours the documented contract.
    unsafe {
        nros_platform_cffi::nros_platform_register_log_writer(Some(writer), None);
    }
}
