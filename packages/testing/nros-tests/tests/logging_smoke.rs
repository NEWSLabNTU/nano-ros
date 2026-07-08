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
    esp32::start_esp32_qemu,
    fixtures::{
        ManagedProcess, QemuProcess, build_logging_smoke_esp32_qemu_flash,
        build_logging_smoke_freertos_mps2, build_logging_smoke_mps2_baremetal,
        build_logging_smoke_nuttx_qemu_arm, build_logging_smoke_threadx_linux,
        build_logging_smoke_threadx_riscv64, build_logging_smoke_zephyr_native_sim,
        build_native_logging, is_arm_toolchain_available, is_qemu_available,
        is_qemu_riscv64_available, nuttx, threadx_linux,
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

    let binary = build_logging_smoke_freertos_mps2().expect(
        "logging-smoke-freertos-mps2 fixture not built — run `just freertos build-fixtures`",
    );

    let mut qemu = QemuProcess::start_mps2_an385_networked(binary)
        .expect("failed to start QEMU (networked slirp)");
    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out waiting for log output");

    assert_output_contains(&output, EXPECTED_LINES);
}

/// Phase 88.15.c — NuttX QEMU ARM virt. NuttX uses the POSIX C
/// platform port (`nros-platform-posix`, shared via the
/// `nros-platform-nuttx` shim); on NuttX that writer routes records
/// through syslog to the configured UART. The harness waits for the
/// final nros-log record itself so the test fails if QEMU capture misses
/// the log path.
#[test]
fn logging_smoke_nuttx_qemu_arm_emits_every_severity() {
    if !nuttx::is_nuttx_available() {
        panic!("[SKIPPED] NuttX source tree not found");
    }
    if !nuttx::is_nuttx_configured() {
        panic!("[SKIPPED] NuttX not configured");
    }
    if !nuttx::is_arm_gcc_available() {
        panic!("[SKIPPED] arm-none-eabi-gcc not on PATH");
    }
    if nuttx::nuttx_kernel_path().is_none() {
        panic!("[SKIPPED] NuttX kernel not built ($NUTTX_DIR/nuttx)");
    }

    let binary = build_logging_smoke_nuttx_qemu_arm()
        .expect("logging-smoke-nuttx-qemu-arm fixture not built — run `just nuttx build-fixtures`");

    // No networking — fixture skips `Executor::open` so no zenoh
    // session is needed. `init_hardware` still runs (5s sleep +
    // ioctl) but the slirp interface is harmless.
    let mut qemu =
        QemuProcess::start_nuttx_virt(binary, true).expect("failed to start QEMU (nuttx-virt)");
    let output = qemu
        .wait_for_output_pattern("[FATAL] smoke: fatal payload", Duration::from_secs(45))
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

    let mut qemu =
        QemuProcess::start_riscv64_virt(binary, 99).expect("failed to start QEMU (riscv64-virt)");
    let output = qemu
        .wait_for_output(Duration::from_secs(30))
        .expect("QEMU timed out waiting for log output");

    assert_output_contains(&output, EXPECTED_LINES);
}

/// ThreadX Linux runs as a host process but uses the ThreadX platform
/// log writer registered by `nros-board-threadx-linux::run()`. The
/// unified host-process harness must drain stderr as well as stdout:
/// `nros-log` records are written to stderr, and this test fails if
/// `ManagedProcess::wait_for_all_output` misses the success pattern.
/// Verifies the ThreadX Linux logging harness captures nano-ros log stderr.
#[test]
fn logging_smoke_harness_captures_stderr() {
    if !threadx_linux::is_threadx_available() {
        panic!("[SKIPPED] THREADX_DIR not set or invalid");
    }
    if !threadx_linux::is_nsos_netx_available() {
        panic!("[SKIPPED] nsos-netx not found at packages/drivers/nsos-netx/");
    }

    let binary = build_logging_smoke_threadx_linux().expect(
        "logging-smoke-threadx-linux fixture not built - run `just threadx_linux build-fixtures`",
    );

    let mut proc = ManagedProcess::spawn(binary, &[], "logging-smoke-threadx-linux")
        .expect("failed to spawn ThreadX Linux logging smoke fixture");
    let output = proc
        .wait_for_all_output(Duration::from_secs(15))
        .expect("ThreadX Linux logging smoke timed out waiting for output");

    assert_output_contains(&output, EXPECTED_LINES);
    assert_output_contains(&output, &["Application completed successfully."]);
}

/// Phase 88.15.f — ESP32-C3 under stock `qemu-system-riscv32 -M
/// esp32c3`. The board crate's `run()` registers an
/// `esp_println`-backed writer with `nros-platform-esp32-qemu`'s
/// fn-ptr slot (Phase 88.15.f groundwork) before the user closure
/// fires; the closure drives every Severity through `nros-log`.
/// ESP32 has no process exit, so the board spins forever after the
/// `nros: application complete` banner (Phase 173.1); the harness
/// drains for the timeout window, then kills QEMU and asserts the
/// expected severity lines.
#[test]
fn logging_smoke_esp32_qemu_emits_every_severity() {
    use std::process::Command;
    if Command::new("qemu-system-riscv32")
        .arg("--version")
        .output()
        .is_err()
    {
        panic!("[SKIPPED] qemu-system-riscv32 not available");
    }

    let flash = build_logging_smoke_esp32_qemu_flash().expect(
        "logging-smoke-esp32-qemu fixture not built — run `just esp32 build-logging-smoke`",
    );

    let mut qemu = start_esp32_qemu(flash, false).expect("failed to start ESP32-C3 QEMU");
    // The fixture drives the severities in order (trace→fatal), so wait for the
    // LAST line: this returns as soon as all six are present (early-return), with
    // a generous ceiling for a slow esp32-qemu boot under CI load. The old fixed
    // 30s window always ran the full 30s and, under load, could expire mid-boot
    // before every severity flushed — the Phase 200.4 "doesn't emit every
    // severity" gap. A real backend regression now fails loudly (no [FATAL]).
    let output = qemu
        .wait_for_output_pattern("[FATAL] smoke: fatal payload", Duration::from_secs(90))
        .expect("ESP32-C3 QEMU: did not emit all severities (no [FATAL] line within 90s)");
    qemu.kill();

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
    let _ = child.wait();

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

/// #102 H3 — the `native/rust/logging` EXAMPLE (host process), covering behavior
/// the `logging-smoke-*` bins do not: a runtime threshold RAISE. It fires every
/// severity in round 1, raises the logger level to `Warn`, then fires them again
/// in round 2 — so trace/debug/info must appear for round 1 but be SUPPRESSED for
/// round 2 while warn/error/fatal survive. Proves `Logger::set_level` filters at
/// dispatch time, end-to-end through the real cffi PlatformSink → stderr chain.
#[test]
fn native_rust_logging_example_threshold_raise_filters_round_two() {
    let binary = build_native_logging()
        .expect("native/rust/logging fixture not built — run `just build-test-fixtures`");

    let mut proc = ManagedProcess::spawn(binary, &[], "native-rs-logging")
        .expect("failed to spawn native/rust/logging example");
    let output = proc
        .wait_for_all_output(Duration::from_secs(15))
        .expect("native/rust/logging timed out waiting for output");

    // Round 1 — every severity fires (level = Trace).
    assert_output_contains(
        &output,
        &[
            "[TRACE] demo: round 1",
            "[DEBUG] demo: round 1",
            "[INFO] demo: round 1",
            "[WARN] demo: round 1",
            "[ERROR] demo: round 1",
            "[FATAL] demo: round 1",
            "-- threshold raised to Warn --",
        ],
    );
    // Round 2 — level = Warn: warn/error/fatal survive.
    assert_output_contains(
        &output,
        &[
            "[WARN] demo: round 2",
            "[ERROR] demo: round 2",
            "[FATAL] demo: round 2",
        ],
    );
    // Round 2 — trace/debug/info MUST be filtered out (the point of the demo).
    for suppressed in [
        "[TRACE] demo: round 2",
        "[DEBUG] demo: round 2",
        "[INFO] demo: round 2",
    ] {
        assert!(
            !output.contains(suppressed),
            "round-2 threshold raise to Warn failed to suppress `{suppressed}`:\n{output}"
        );
    }
}
