//! phase-263 C2c — runtime E2E for the C++ embedded workspace entry on ThreadX-on-Linux,
//! `nano_ros_entry(BOARD threadx-linux LAUNCH …)` in the C++ workspace. The C++ sibling of
//! C2a's C entry — same per-board wiring (locator bake, header-mirror ordering) reused
//! verbatim through the C++ emitter.
//!
//! Delivery is cross-process (issue 0096): the threadx C++ entry runs the `demo_bringup`
//! talker, and a SEPARATE native listener (the C `native_entry_robot2`, a language-agnostic
//! `/chatter` subscriber on the wire) receives it through a zenoh router. The entry's
//! locator is compile-time baked (`tcp/127.0.0.1:17803`), so the router uses that exact
//! port; nsos-netx forwards to a host `connect()` — no veth bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_threadx_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_entry_robot2,
    build_threadx_linux_workspace_cpp_entry, require_zenohd,
    threadx_linux::{is_nsos_netx_available, is_threadx_available},
};
use std::{process::Command, time::Duration};

/// The router port baked into the C++ threadx entry's locator (see the
/// `workspace-cpp-threadx-linux` fixture's `NROS_ENTRY_LOCATOR`).
const THREADX_ENTRY_PORT: u16 = 17803;

#[test]
fn cpp_threadx_linux_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_threadx_available() {
        nros_tests::skip!("THREADX_DIR not set or invalid");
    }
    if !is_nsos_netx_available() {
        nros_tests::skip!("nsos-netx not found at packages/drivers/nsos-netx/");
    }

    let entry = build_threadx_linux_workspace_cpp_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("threadx-linux C++ entry fixture not built: {e}"));
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
        ManagedProcess::spawn_command(cmd, "threadx-cpp-entry")
            .unwrap_or_else(|e| panic!("spawn threadx C++ entry: {e}"))
    };

    let out = obs
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(20),
        )
        .unwrap_or_else(|_| {
            tx.kill();
            obs.kill();
            panic!(
                "native observer never received the threadx-linux C++ entry's /chatter — \
                 the embedded C++ LAUNCH-entry runtime delivery did not work"
            )
        });

    tx.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
