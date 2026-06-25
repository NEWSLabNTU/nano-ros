//! phase-263 C2a — runtime E2E for the FIRST C/C++ workspace EMBEDDED entry: the
//! ThreadX-on-Linux host sim driven by `nano_ros_entry(BOARD threadx-linux LAUNCH …)`.
//!
//! This proves the embedded LAUNCH-entry path (issue 0097) end-to-end. The codegen
//! emits `nros_app_main` + `ThreadxBoard::run_components` (NOT `int main`, which would
//! double-main the ThreadX `startup.c`); the cmake `nano_ros_entry` embedded pass links
//! the board startup/kernel/netstack, bakes the connect locator, and orders the
//! generated TU after the per-build sizes-header mirror. The booted ELF *is* the ThreadX
//! kernel: `startup.c::main → tx_kernel_enter →` app thread `→ app_main →`
//! `ThreadxBoard::run_components → nros::init(locator) →` spin.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096 — two nodes in one zenoh session do
//! not deliver to each other): the threadx entry runs the `demo_bringup` talker, and a
//! SEPARATE native C listener entry (`native_entry_robot2`, listener-only) subscribes
//! `/chatter` through the same router. The threadx entry's locator is COMPILE-TIME baked
//! (`tcp/127.0.0.1:17553`, embedded domain/locator rule — never a runtime env), so the
//! router must listen on exactly that port; the native observer takes it via the runtime
//! `NROS_LOCATOR`. nsos-netx forwards `nx_bsd_connect` to a host `connect()`, so the sim
//! reaches loopback with NO veth bridge / root (unlike the 192.0.3.x bridge the Rust
//! threadx examples dial).
//!
//! Run with: `cargo nextest run -p nros-tests --test c_threadx_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_entry_robot2,
    build_threadx_linux_workspace_c_entry, require_zenohd,
    threadx_linux::{is_nsos_netx_available, is_threadx_available},
};
use std::{process::Command, time::Duration};

/// The router port baked into the threadx entry's locator (see the
/// `workspace-c-threadx-linux` fixture's `NROS_ENTRY_LOCATOR`). The embedded
/// locator is fixed at build time, so the router cannot use an ephemeral port.
const THREADX_ENTRY_PORT: u16 = 17553;

/// C2a — the embedded threadx-linux LAUNCH entry boots the ThreadX kernel, brings the
/// nros runtime online over the baked loopback locator, and its talker delivers
/// `/chatter` to a separate native listener process through the router.
#[test]
fn threadx_linux_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_threadx_available() {
        nros_tests::skip!("THREADX_DIR not set or invalid");
    }
    if !is_nsos_netx_available() {
        nros_tests::skip!("nsos-netx not found at packages/drivers/nsos-netx/");
    }

    let entry = build_threadx_linux_workspace_c_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("threadx-linux C entry fixture not built: {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native C listener entry fixture not built: {e}"));

    // Router on the exact port the entry's locator was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", THREADX_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {THREADX_ENTRY_PORT}: {e}")
    });
    let locator = router.locator();

    // External observer (native listener) first, so its subscription is live before the
    // embedded talker publishes. It takes the locator at RUNTIME via NROS_LOCATOR.
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

    // The embedded threadx entry. Its locator is COMPILE-TIME baked, so NROS_LOCATOR is
    // ignored here; only the bounded spin is threaded so the process exits.
    let mut tx = {
        let mut cmd = Command::new(&entry);
        cmd.env("NROS_ENTRY_SPIN_MS", "12000");
        ManagedProcess::spawn_command(cmd, "threadx-entry")
            .unwrap_or_else(|e| panic!("spawn threadx entry: {e}"))
    };

    // The observer prints `Received: <n>` per delivered message — 3 confirms the embedded
    // entry's talker reached a separate process through the router.
    let out = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(20))
        .unwrap_or_else(|_| {
            tx.kill();
            obs.kill();
            panic!(
                "native observer never received the threadx-linux embedded entry's \
                 /chatter — the embedded LAUNCH-entry runtime delivery did not work"
            )
        });

    tx.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
