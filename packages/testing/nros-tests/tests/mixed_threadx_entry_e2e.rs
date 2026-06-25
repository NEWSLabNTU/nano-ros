//! phase-263 C2c — runtime E2E for the MIXED (C + C++ + Rust) embedded workspace entry on
//! ThreadX-on-Linux, `nano_ros_entry(BOARD threadx-linux LAUNCH …)` in the mixed workspace.
//!
//! The mixed workspace runs a C talker, a C++ listener, AND a Rust heartbeat node in one
//! bootable image — the Rust node linked via the `nros_ws_runtime` umbrella, which on
//! threadx-linux targets the host x86_64 triple (the ThreadX sim runs as pthreads), so the
//! Rust node compiles host-side like the native mixed entry. Same per-board embedded wiring
//! (locator bake, header ordering) reused from C2a/C2b.
//!
//! Delivery is cross-process (issue 0096): the entry's C talker publishes `/chatter`, and a
//! SEPARATE native listener (the C `native_entry_robot2`) receives it through a zenoh
//! router. The locator is compile-time baked (`tcp/127.0.0.1:17821`).
//!
//! Run with: `cargo nextest run -p nros-tests --test mixed_threadx_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_entry_robot2,
    build_threadx_linux_workspace_mixed_entry, require_zenohd,
    threadx_linux::{is_nsos_netx_available, is_threadx_available},
};
use std::{process::Command, time::Duration};

/// The router port baked into the mixed threadx entry's locator (see the
/// `workspace-mixed-threadx-linux` fixture's `NROS_ENTRY_LOCATOR`).
const THREADX_ENTRY_PORT: u16 = 17821;

#[test]
fn mixed_threadx_linux_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_threadx_available() {
        nros_tests::skip!("THREADX_DIR not set or invalid");
    }
    if !is_nsos_netx_available() {
        nros_tests::skip!("nsos-netx not found at packages/drivers/nsos-netx/");
    }

    let entry = build_threadx_linux_workspace_mixed_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("threadx-linux mixed entry fixture not built: {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    let router = ZenohRouter::start_on("127.0.0.1", THREADX_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {THREADX_ENTRY_PORT}: {e}")
    });
    let locator = router.locator();

    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("NROS_LOCATOR", &locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", "15000");
        ManagedProcess::spawn_command(cmd, "native-observer")
            .unwrap_or_else(|e| panic!("spawn observer: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for messages", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native observer listener never became ready")
        });

    let mut tx = {
        let mut cmd = Command::new(&entry);
        cmd.env("NROS_ENTRY_SPIN_MS", "12000");
        ManagedProcess::spawn_command(cmd, "mixed-threadx-entry")
            .unwrap_or_else(|e| panic!("spawn mixed threadx entry: {e}"))
    };

    let out = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(20))
        .unwrap_or_else(|_| {
            tx.kill();
            obs.kill();
            panic!(
                "native observer never received the mixed threadx-linux entry's /chatter — \
                 the embedded mixed (C+C++/Rust) LAUNCH-entry runtime delivery did not work"
            )
        });

    tx.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
