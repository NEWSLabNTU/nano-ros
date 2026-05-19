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

use std::time::Duration;

use nros_tests::{
    assert_output_contains,
    fixtures::{
        build_logging_smoke_mps2_baremetal, is_arm_toolchain_available, is_qemu_available,
        QemuProcess,
    },
};

/// Lines the fixture must produce, in order. The mps2-an385
/// `PlatformLog` writer formats every record as
/// `[<LEVEL>] <name>: <message>\n` — see
/// `packages/platforms/nros-platform-mps2-an385/src/lib.rs`. The
/// writer routes to `hstderr()`, which Phase 88.16.A teaches
/// `QemuProcess::wait_for_output` to drain alongside stdout.
const EXPECTED_LINES: &[&str] = &[
    "[TRACE] smoke: trace payload",
    "[DEBUG] smoke: debug payload",
    "[INFO] smoke: info payload",
    "[WARN] smoke: warn payload",
    "[ERROR] smoke: error payload",
    "[FATAL] smoke: fatal payload",
];

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

    let mut qemu = QemuProcess::start_mps2_an385(binary).expect("failed to start QEMU");
    let output = qemu
        .wait_for_output(Duration::from_secs(15))
        .expect("QEMU timed out waiting for log output");

    assert_output_contains(&output, EXPECTED_LINES);
}
