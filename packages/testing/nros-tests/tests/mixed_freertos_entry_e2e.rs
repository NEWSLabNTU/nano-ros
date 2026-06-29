//! phase-263 C2c — runtime E2E for the MIXED (C + C++ + no_std Rust) embedded workspace
//! entry on FreeRTOS QEMU MPS2-AN385. The embedded sibling of the threadx-linux mixed entry,
//! but on a GENUINELY-no_std cross target (thumbv7m-none-eabi) — proving the no_std Rust node
//! and the cross-compiled `nros_ws_runtime` umbrella bundle into the bootable FreeRTOS image
//! beside the C talker and C++ listener.
//!
//! The `rust_heartbeat_pkg` node is `#![no_std]` (nros alloc-only) and `rlib`-only; the
//! umbrella, on a cross build, selects the board's `alloc;panic-halt` tier (not `std`) and is
//! itself `#![no_std]`, so Corrosion cross-compiles it for thumbv7m and re-points
//! NanoRos::NanoRosCpp. (The host umbrella keeps `std`, so the threadx-linux mixed entry is
//! unaffected.)
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the QEMU guest runs the `demo_bringup`
//! talker, and a SEPARATE native C listener (`native_entry_robot2`) receives `/chatter`
//! through a host zenohd over the board-matching slirp net (host 192.0.3.1, baked locator
//! `tcp/192.0.3.1:17841`). No TAP / bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test mixed_freertos_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_freertos_workspace_mixed_entry,
    build_native_workspace_c_entry_robot2, freertos, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the mixed freertos entry's locator (see the
/// `workspace-mixed-freertos` fixture's `NROS_ENTRY_LOCATOR`).
const FREERTOS_ENTRY_PORT: u16 = 17841;

#[test]
fn mixed_freertos_entry_delivers_cross_process() {
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

    let entry = build_freertos_workspace_mixed_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("mixed freertos entry fixture not built: {e}"));
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
        .wait_for_output_count("Received:", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native observer never received the mixed FreeRTOS QEMU entry's /chatter — \
                 the embedded mixed (C+C++/no_std-Rust) runtime delivery did not work"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
