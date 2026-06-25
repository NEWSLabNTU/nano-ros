//! phase-263 C2b — runtime E2E for the FIRST QEMU-cross C/C++ workspace EMBEDDED entry:
//! FreeRTOS on QEMU MPS2-AN385, driven by `nano_ros_entry(BOARD mps2-an385-freertos
//! LAUNCH …)`. The FreeRTOS sibling to C2a's threadx-linux host sim.
//!
//! Proves the embedded LAUNCH-entry path (issue 0097) on a cross-compiled QEMU target.
//! The codegen emits `nros_app_main` + `FreertosBoard::run_components` (NOT `int main`);
//! the cmake `nano_ros_entry` embedded pass links the bootable ELF (FreeRTOS kernel +
//! lwIP + LAN9118 + startup), generates the `NROS_APP_CONFIG` TU startup.c reads, bakes
//! the connect locator, and orders the generated TU after the sizes-header mirror. The
//! board's `startup.c` `_start` spawns the app task + starts the scheduler; that task
//! brings up the netif + zenoh/poll tasks, then dispatches to `app_main`.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the QEMU guest runs the `demo_bringup`
//! talker, and a SEPARATE native C listener entry (`native_entry_robot2`) subscribes
//! `/chatter` through a host zenohd. The firmware uses a STATIC `192.0.3.x` lwIP config
//! (no DHCP — `startup.c`), so QEMU slirp is configured with a matching net whose host
//! (`192.0.3.1`, the board's gateway) forwards to the host machine; the entry dials
//! `tcp/192.0.3.1:<port>` (compile-time baked). No TAP / bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_freertos_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, QemuProcess, ZenohRouter, build_freertos_workspace_c_entry,
    build_native_workspace_c_entry_robot2, freertos, is_qemu_available, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the freertos entry's locator (see the `workspace-c-freertos`
/// fixture's `NROS_ENTRY_LOCATOR`). The embedded locator is fixed at build time.
const FREERTOS_ENTRY_PORT: u16 = 17601;

/// C2b — the embedded FreeRTOS QEMU entry boots the kernel, brings the nros runtime online
/// over the baked slirp locator, and its talker delivers `/chatter` to a separate native
/// listener process through the host router.
#[test]
fn freertos_entry_delivers_cross_process() {
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

    let entry = build_freertos_workspace_c_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("freertos C entry fixture not built: {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native C listener entry fixture not built: {e}"));

    // Router bound on 0.0.0.0 so the slirp guest (forwarded via host 192.0.3.1) reaches
    // it; the native observer dials the same port on loopback.
    let router = ZenohRouter::start_on("0.0.0.0", FREERTOS_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {FREERTOS_ENTRY_PORT}: {e}")
    });
    let observer_locator = format!("tcp/127.0.0.1:{FREERTOS_ENTRY_PORT}");
    let _ = router;

    // External observer (native listener) first, so its subscription is live before the
    // QEMU guest's talker publishes.
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

    // Boot the FreeRTOS firmware in QEMU with the board-matching slirp net. The image
    // runs until killed (no bounded spin on the embedded target).
    let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot freertos QEMU: {e}"));

    // The observer prints `Received: <n>` per delivered message — 3 confirms the QEMU
    // guest's talker reached a separate process through the router. QEMU cold boot +
    // lwIP + zenoh connect is slow, so allow a generous window.
    let out = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            qemu.kill();
            obs.kill();
            panic!(
                "native observer never received the FreeRTOS QEMU entry's /chatter — \
                 the embedded LAUNCH-entry QEMU runtime delivery did not work"
            )
        });

    qemu.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
