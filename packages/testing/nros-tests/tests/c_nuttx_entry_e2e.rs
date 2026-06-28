//! phase-263 C2b — runtime E2E for the NuttX (QEMU arm-virt) C workspace EMBEDDED entry:
//! `nano_ros_entry(BOARD nuttx-qemu-arm LAUNCH …)` — a C talker + C listener linked INTO the
//! NuttX kernel (the bootable `armv7a-nuttx-eabihf` ELF, cargo `nros-nuttx-ffi` build). The
//! NuttX sibling of the threadx/freertos/zephyr C entries.
//!
//! This closes the LAST C2 runtime gap. The earlier "host-wide nuttx-qemu console issue" was a
//! MISDIAGNOSIS — the console works; the real gap was the LAUNCH path not baking the connect
//! locator into the kernel (it defaulted to `tcp/127.0.0.1:7447` → no router → ConnectionFailed
//! → looked silent). Fixed by setting `NROS_ENTRY_LOCATOR` on the entry target BEFORE the board's
//! `nros_platform_link_app`, which on NuttX ferries the target's COMPILE_DEFINITIONS into the
//! cc-rs entry-TU compile at CONFIGURE time.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096 — the entry's own listener can't receive the
//! talker's in-process query): the QEMU guest runs the `demo_bringup` talker, and a SEPARATE
//! native C listener (`native_entry_robot2`, a language-agnostic `/chatter` subscriber) receives
//! it through a host zenoh router. The guest dials the router through the QEMU slirp gateway
//! (10.0.2.2 → host) at the baked port (17861); the observer dials 127.0.0.1:17861. No
//! TAP/bridge/root.
//!
//! The fixture is built by `just nuttx build-examples`; this test skips cleanly when the NuttX
//! kernel ELF is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_nuttx_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_native_workspace_c_entry_robot2,
    build_nuttx_workspace_c_entry, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the NuttX C workspace entry's locator (see the `workspace-c-nuttx`
/// fixture's `NROS_ENTRY_LOCATOR = "tcp/10.0.2.2:17861"`).
const NUTTX_ENTRY_PORT: u16 = 17861;

#[test]
fn c_nuttx_workspace_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_nuttx_workspace_c_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("NuttX C workspace entry not built: {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    let router = ZenohRouter::start_on("0.0.0.0", NUTTX_ENTRY_PORT)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {NUTTX_ENTRY_PORT}: {e}"));
    let observer_locator = format!("tcp/127.0.0.1:{NUTTX_ENTRY_PORT}");
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

    let mut qemu = QemuProcess::start_nuttx_virt(&entry, true)
        .unwrap_or_else(|e| panic!("boot NuttX QEMU: {e}"));

    let out = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native observer never received the NuttX QEMU entry's /chatter — \
                 the embedded multi-node (C talker + listener) NuttX runtime delivery did not work"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
