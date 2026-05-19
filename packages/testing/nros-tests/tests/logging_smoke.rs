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

use std::{
    process::{Command, Stdio},
    time::Instant,
};

use nros_tests::{
    assert_output_contains,
    fixtures::{
        build_logging_smoke_freertos_mps2, build_logging_smoke_mps2_baremetal,
        build_logging_smoke_threadx_riscv64, build_logging_smoke_zephyr_native_sim,
        is_arm_toolchain_available, is_qemu_available, is_qemu_riscv64_available, QemuProcess,
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

/// Phase 88.15.b — MPS2-AN385 + FreeRTOS + lwIP over QEMU
/// semihosting. The board crate's `run()` registers a semihosting
/// writer with `nros-platform-freertos`'s fn-ptr slot (Phase 88.11)
/// before the user closure fires; the closure then drives every
/// severity through `nros-log`.
#[test]
fn logging_smoke_freertos_mps2_emits_every_severity() {
    if !is_qemu_available() {
        panic!("[SKIPPED] qemu-system-arm not available");
    }
    if !is_arm_toolchain_available() {
        panic!("[SKIPPED] thumbv7m-none-eabi target not installed");
    }

    let binary = build_logging_smoke_freertos_mps2()
        .expect("logging-smoke-freertos-mps2 fixture not built — run `just freertos build-fixtures`");

    let mut qemu = QemuProcess::start_mps2_an385_networked(binary)
        .expect("failed to start QEMU (networked slirp)");
    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out waiting for log output");

    assert_output_contains(&output, EXPECTED_LINES);
}

/// Phase 88.15.d — ThreadX + NetX Duo on QEMU RISC-V `virt`. The
/// board crate's `run()` registers a UART writer with
/// `nros-platform-threadx`'s fn-ptr slot (Phase 88.11) before the
/// user closure fires; the closure drives every severity and exits
/// through the QEMU `test-finisher` MMIO device.
#[test]
fn logging_smoke_threadx_riscv64_emits_every_severity() {
    if !is_qemu_riscv64_available() {
        panic!("[SKIPPED] qemu-system-riscv64 not available");
    }

    let binary = build_logging_smoke_threadx_riscv64().expect(
        "logging-smoke-threadx-riscv64 fixture not built — run `just threadx_riscv64 build-fixtures`",
    );

    let mut qemu = QemuProcess::start_riscv64_virt(binary, 99)
        .expect("failed to start QEMU (riscv64-virt)");
    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out waiting for log output");

    assert_output_contains(&output, EXPECTED_LINES);
}

/// Phase 88.15.e — Zephyr `native_sim/native/64` running as a Linux
/// process. The platform's `PlatformLog` impl
/// (`nros-platform-zephyr::nros_platform_log_write`) routes each
/// nros record through Zephyr's `LOG_INF` / `LOG_WRN` / `LOG_ERR`
/// macros. Zephyr's runtime LOG filter blocks `LOG_DBG` records
/// below `CONFIG_LOG_DEFAULT_LEVEL`, so the smoke checks only the
/// four severities that survive the standard logging level.
#[test]
fn logging_smoke_zephyr_native_sim_emits_every_severity() {
    let binary = build_logging_smoke_zephyr_native_sim().expect(
        "logging-smoke-zephyr-native-sim fixture not built — run `just zephyr build-logging-smoke`",
    );

    let mut child = Command::new(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn native_sim binary");

    // Drain both fds in parallel — Zephyr LOG backend may pick
    // either stream depending on the native_sim console driver.
    use std::io::Read;
    let mut stdout = child.stdout.take().expect("no stdout");
    let mut stderr = child.stderr.take().expect("no stderr");
    let mut output = String::new();
    let start = Instant::now();
    let mut buf = [0u8; 4096];
    let deadline = Duration::from_secs(15);
    while start.elapsed() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            let _ = stdout.read_to_string(&mut output);
            let _ = stderr.read_to_string(&mut output);
            break;
        }
        if let Ok(n) = stdout.read(&mut buf)
            && n > 0
        {
            output.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        if let Ok(n) = stderr.read(&mut buf)
            && n > 0
        {
            output.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();

    // The Zephyr LOG default filter drops DBG records below the
    // configured CONFIG_LOG_DEFAULT_LEVEL, so the platform impl's
    // TRACE / DEBUG -> LOG_DBG mapping doesn't surface unless the
    // user bumps CONFIG_LOG_MAX_LEVEL. Check the four severities
    // that pass the runtime gate.
    assert_output_contains(
        &output,
        &[
            "info payload",
            "warn payload",
            "error payload",
            "fatal payload",
        ],
    );
}
