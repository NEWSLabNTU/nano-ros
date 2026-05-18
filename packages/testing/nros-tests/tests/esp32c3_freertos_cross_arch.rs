//! Phase 23.5c — ESP32-C3 ↔ ARM Cortex-M3 cross-architecture QEMU
//! interop through a single zenohd. Two QEMU machines (Espressif
//! ESP32-C3 RISC-V + MPS2-AN385 ARM Cortex-M3) each on their own
//! slirp NAT both connect to a host-side zenohd that listens on
//! both `platform::ESP32.zenohd_port` and
//! `platform::FREERTOS.zenohd_port`; zenohd routes /chatter
//! Int32s between them.
//!
//! Validates the wire-format compatibility of the same nros stack
//! (zenoh-pico + nros-rmw + CDR Int32 on the canonical
//! `rt//chatter/std_msgs/msg/dds_/String_/RIHS01_…` keyexpr) across
//! a RISC-V firmware and an ARM firmware. The two test directions
//! mirror the Phase 23.5c spec rows.
//!
//! Skip behavior matches the per-side tests (`esp32_emulator.rs`,
//! `emulator.rs`): if either ESP-IDF QEMU or qemu-system-arm or
//! zenohd is missing, the test is skipped via `nros_tests::skip!`.

use nros_tests::{
    count_pattern,
    esp32::start_esp32_qemu,
    fixtures::{
        build_esp32_qemu_listener, build_esp32_qemu_talker,
        freertos::{
            build_freertos_listener, build_freertos_talker, is_arm_gcc_available,
            is_freertos_available, is_lwip_available,
        },
        is_qemu_available, is_zenohd_available, zenohd_binary_path,
    },
    platform,
    qemu::QemuProcess,
    wait_for_port,
};
use std::{path::PathBuf, process::Stdio, time::Duration};

// =============================================================================
// Setup helpers
// =============================================================================

fn require_cross_arch() -> bool {
    if !is_zenohd_available() {
        eprintln!("Skipping: zenohd unavailable");
        return false;
    }
    if !is_qemu_available() {
        eprintln!("Skipping: qemu-system-* unavailable");
        return false;
    }
    if !is_arm_gcc_available() {
        eprintln!("Skipping: arm-none-eabi-gcc unavailable");
        return false;
    }
    if !is_freertos_available() || !is_lwip_available() {
        eprintln!("Skipping: FREERTOS_DIR / LWIP_DIR not set");
        return false;
    }
    if !nros_tests::esp32::require_riscv32_target() {
        eprintln!("Skipping: riscv32imc target not installed");
        return false;
    }
    if std::process::Command::new("qemu-system-riscv32")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| !s.success())
        .unwrap_or(true)
    {
        eprintln!("Skipping: qemu-system-riscv32 (Espressif fork) unavailable");
        return false;
    }
    true
}

/// Start a zenohd listening on BOTH the ESP32 slirp port and the
/// FreeRTOS slirp port. Returns the child handle for the caller to
/// kill on drop. Both ports are killed first so an orphaned
/// zenohd from a previous run does not block us.
fn start_dual_listen_zenohd() -> std::process::Child {
    let esp_port = platform::ESP32.zenohd_port;
    let freertos_port = platform::FREERTOS.zenohd_port;
    // Best-effort orphan cleanup via fuser / lsof — kill_listeners_on_port
    // is private to the zenohd_router fixture; this fallback is good
    // enough for serial test runs.
    for port in [esp_port, freertos_port] {
        let _ = std::process::Command::new("fuser")
            .args(["-k", &format!("{port}/tcp")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let mut cmd = std::process::Command::new(zenohd_binary_path());
    cmd.args([
        "--listen",
        &format!("tcp/0.0.0.0:{esp_port}"),
        "--listen",
        &format!("tcp/0.0.0.0:{freertos_port}"),
        "--no-multicast-scouting",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    let child = cmd.spawn().expect("spawn dual-listen zenohd");
    // Wait for both ports to accept connections.
    assert!(
        wait_for_port(esp_port, Duration::from_secs(10)),
        "dual zenohd never listened on ESP32 port {esp_port}"
    );
    assert!(
        wait_for_port(freertos_port, Duration::from_secs(10)),
        "dual zenohd never listened on FREERTOS port {freertos_port}"
    );
    child
}

struct DualZenohd(std::process::Child);
impl Drop for DualZenohd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn build_esp32_talker_flash() -> PathBuf {
    let elf = build_esp32_qemu_talker().expect("build esp32-qemu-talker");
    let root = nros_tests::project_root();
    let bin = root.join("build/esp32-qemu/esp32-qemu-talker.bin");
    nros_tests::esp32::create_esp32_flash_image(elf, &bin).expect("flash image");
    bin
}

fn build_esp32_listener_flash() -> PathBuf {
    let elf = build_esp32_qemu_listener().expect("build esp32-qemu-listener");
    let root = nros_tests::project_root();
    let bin = root.join("build/esp32-qemu/esp32-qemu-listener.bin");
    nros_tests::esp32::create_esp32_flash_image(elf, &bin).expect("flash image");
    bin
}

// =============================================================================
// Cross-arch tests
// =============================================================================

/// ESP32-C3 (RISC-V) talker → ARM Cortex-M3 (FreeRTOS) listener
/// through a single host-side zenohd.
///
/// `#[ignore]` until the upstream
/// `examples/qemu-esp32-baremetal/rust/zenoh/talker` firmware stops
/// faulting in the OpenETH / smoltcp init path under
/// `qemu-system-riscv32 -machine esp32c3`. Tracked under
/// Phase 89.4 follow-up (same root cause that makes
/// `test_esp32_to_native` in `esp32_emulator.rs` skip). The
/// cross-arch test wiring itself (dual-listen zenohd, two QEMUs,
/// pattern-watch assertions) works end-to-end against the
/// FreeRTOS-ARM side; ESP32-C3 firmware is the only failing
/// component.
#[test]
#[ignore]
fn test_esp32c3_to_freertos() {
    if !require_cross_arch() {
        nros_tests::skip!("cross-arch prerequisites missing");
    }
    let talker_bin = build_esp32_talker_flash();
    let listener_elf = build_freertos_listener().expect("build freertos listener");
    let _zenohd = DualZenohd(start_dual_listen_zenohd());

    eprintln!("Starting FreeRTOS ARM listener (mps2-an385 slirp)…");
    let mut listener = QemuProcess::start_mps2_an385_networked(listener_elf)
        .expect("start FreeRTOS listener QEMU");
    let _ = listener
        .wait_for_output_pattern("Waiting for messages...", Duration::from_secs(60))
        .expect("FreeRTOS listener never reached subscribe-ready");

    // Stabilization — subscription must propagate to zenohd before
    // the talker starts publishing or early samples may drop.
    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting ESP32-C3 talker QEMU (esp32c3 slirp)…");
    let mut talker =
        start_esp32_qemu(&talker_bin, true).expect("start ESP32-C3 talker QEMU");
    let talker_output = talker
        .wait_for_output_pattern("Published:", Duration::from_secs(60))
        .expect("ESP32-C3 talker never published");

    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(30))
        .unwrap_or_default();
    let received = count_pattern(&listener_output, "Received:");
    let published = count_pattern(&talker_output, "Published:");
    eprintln!(
        "cross-arch ESP32-C3 → FreeRTOS: published={published}, received={received}"
    );
    assert!(
        received >= 1,
        "FreeRTOS listener received zero ESP32-C3 messages\n--- listener ---\n{listener_output}\n--- talker ---\n{talker_output}"
    );
}

/// FreeRTOS ARM Cortex-M3 talker → ESP32-C3 (RISC-V) listener
/// through a single host-side zenohd. `#[ignore]` for the same
/// reason as `test_esp32c3_to_freertos` — see that test's
/// docstring.
#[test]
#[ignore]
fn test_freertos_to_esp32c3() {
    if !require_cross_arch() {
        nros_tests::skip!("cross-arch prerequisites missing");
    }
    let talker_elf = build_freertos_talker().expect("build freertos talker");
    let listener_bin = build_esp32_listener_flash();
    let _zenohd = DualZenohd(start_dual_listen_zenohd());

    eprintln!("Starting ESP32-C3 listener QEMU (esp32c3 slirp)…");
    let mut listener =
        start_esp32_qemu(&listener_bin, true).expect("start ESP32-C3 listener QEMU");
    let _ = listener
        .wait_for_output_pattern("Waiting for messages...", Duration::from_secs(60))
        .expect("ESP32-C3 listener never reached subscribe-ready");

    std::thread::sleep(Duration::from_secs(5));

    eprintln!("Starting FreeRTOS ARM talker (mps2-an385 slirp)…");
    let mut talker = QemuProcess::start_mps2_an385_networked(talker_elf)
        .expect("start FreeRTOS talker QEMU");
    let talker_output = talker
        .wait_for_output_pattern("Published:", Duration::from_secs(60))
        .expect("FreeRTOS talker never published");

    let listener_output = listener
        .wait_for_output_pattern("Received:", Duration::from_secs(30))
        .unwrap_or_default();
    let received = count_pattern(&listener_output, "Received:");
    let published = count_pattern(&talker_output, "Published:");
    eprintln!(
        "cross-arch FreeRTOS → ESP32-C3: published={published}, received={received}"
    );
    assert!(
        received >= 1,
        "ESP32-C3 listener received zero FreeRTOS messages\n--- listener ---\n{listener_output}\n--- talker ---\n{talker_output}"
    );
}
