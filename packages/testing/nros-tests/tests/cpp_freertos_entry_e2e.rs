//! phase-263 C2c — runtime E2E for the C++ embedded workspace entry on FreeRTOS (QEMU
//! MPS2-AN385), `nano_ros_entry(BOARD mps2-an385-freertos LAUNCH …)` in the C++ workspace.
//! The C++ sibling of C2b's C entry — same per-board wiring (locator bake, NROS_APP_CONFIG
//! TU, header ordering, cross toolchain, board-matching slirp net) reused verbatim through
//! the C++ emitter.
//!
//! Delivery is cross-process (issue 0096): the QEMU guest runs the `demo_bringup` talker,
//! and a SEPARATE native listener (the C `native_entry_robot2`, a language-agnostic
//! `/chatter` subscriber on the wire) receives it through a host zenohd. The firmware uses a
//! static 192.0.3.x lwIP config, so QEMU slirp runs a matching net (host 192.0.3.1) and the
//! entry dials `tcp/192.0.3.1:17811` (baked). No TAP / bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_freertos_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_freertos_workspace_cpp_entry,
    build_native_workspace_c_entry_robot2, freertos, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C++ freertos entry's locator (see the
/// `workspace-cpp-freertos` fixture's `NROS_ENTRY_LOCATOR`).
const FREERTOS_ENTRY_PORT: u16 = 17811;

#[test]
fn cpp_freertos_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !freertos::is_freertos_available() {
        nros_tests::skip!("FREERTOS_DIR not set or invalid");
    }
    if !freertos::is_lwip_available() {
        nros_tests::skip!("LWIP_DIR not set or invalid");
    }
    if !freertos::is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_freertos_workspace_cpp_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("freertos C++ entry fixture not built: {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", FREERTOS_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {FREERTOS_ENTRY_PORT}: {e}")
    });
    let observer_locator = format!("tcp/127.0.0.1:{FREERTOS_ENTRY_PORT}");
    let _ = router;

    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("NROS_LOCATOR", &observer_locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", "60000");
        ManagedProcess::spawn_command(cmd, "native-observer")
            .unwrap_or_else(|e| panic!("spawn observer: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for messages", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native observer listener never became ready")
        });

    let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot freertos QEMU: {e}"));

    let out = obs
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native observer never received the FreeRTOS QEMU C++ entry's /chatter — \
                 the embedded C++ LAUNCH-entry QEMU runtime delivery did not work"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
