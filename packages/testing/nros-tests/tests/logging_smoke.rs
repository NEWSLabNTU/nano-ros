//! Phase 88.15 — RTOS smoke fixtures + QEMU output capture.
//!
//! Each test boots a minimal `logging-smoke-<platform>` fixture binary
//! under QEMU and asserts the rendered `[TRACE]`/`[DEBUG]`/`[INFO]`/
//! `[WARN]`/`[ERROR]`/`[FATAL]` lines reach the captured UART or
//! semihosting output. The chain under test is:
//!
//! `nros-log` macro → `Logger::dispatch` → `PlatformSink` →
//! `nros_platform_log_write` (cffi) → per-platform writer.
//!
//! Fixtures must be prebuilt — run `just qemu build-fixtures` for the
//! bare-metal MPS2-AN385 slice.

use std::{
    io::Read,
    process::Stdio,
    time::{Duration, Instant},
};

use nros_tests::{
    assert_output_contains,
    fixtures::{build_logging_smoke_mps2_baremetal, is_arm_toolchain_available, is_qemu_available},
    qemu::qemu_system_arm_cmd,
};

/// Lines the fixture must produce, in order. The mps2-an385
/// `PlatformLog` writer formats every record as
/// `[<LEVEL>] <name>: <message>\n` — see
/// `packages/platforms/nros-platform-mps2-an385/src/lib.rs`. The
/// writer routes to `hstderr()`, so the harness has to drain stderr
/// (the standard `QemuProcess::wait_for_output` only reads stdout).
const EXPECTED_LINES: &[&str] = &[
    "[TRACE] smoke: trace payload",
    "[DEBUG] smoke: debug payload",
    "[INFO] smoke: info payload",
    "[WARN] smoke: warn payload",
    "[ERROR] smoke: error payload",
    "[FATAL] smoke: fatal payload",
];

/// Spawn the fixture under QEMU MPS2-AN385, merging stderr →
/// stdout, and wait for the process to exit (or `timeout`). Returns
/// the captured combined output.
fn run_under_qemu_mps2_an385(binary: &std::path::Path, timeout: Duration) -> String {
    let mut cmd = qemu_system_arm_cmd();
    cmd.args([
        "-cpu",
        "cortex-m3",
        "-machine",
        "mps2-an385",
        "-nographic",
        "-semihosting-config",
        "enable=on,target=native",
        "-kernel",
    ])
    .arg(binary)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("failed to spawn qemu-system-arm");
    let mut stderr = child.stderr.take().expect("no stderr");
    let mut output = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];

    while start.elapsed() < timeout {
        match child.try_wait() {
            Ok(Some(_)) => {
                let _ = stderr.read_to_string(&mut output);
                return output;
            }
            Ok(None) => {
                match stderr.read(&mut buf) {
                    Ok(0) => std::thread::sleep(Duration::from_millis(20)),
                    Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    output
}

/// Phase 88.15.a — bare-metal MPS2-AN385 over QEMU semihosting.
#[test]
fn logging_smoke_mps2_baremetal_emits_every_severity() {
    if !is_qemu_available() {
        panic!("[SKIPPED] qemu-system-arm not available");
    }
    if !is_arm_toolchain_available() {
        panic!("[SKIPPED] thumbv7m-none-eabi target not installed");
    }

    let binary = build_logging_smoke_mps2_baremetal()
        .expect("logging-smoke-mps2-baremetal fixture not built — run `just qemu build-fixtures`");

    let output = run_under_qemu_mps2_an385(binary, Duration::from_secs(15));
    assert_output_contains(&output, EXPECTED_LINES);
}
