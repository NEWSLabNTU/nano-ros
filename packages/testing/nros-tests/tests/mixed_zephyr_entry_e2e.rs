//! phase-263 C2c-zephyr — runtime E2E for the Zephyr (native_sim) MIXED workspace EMBEDDED
//! entry: a C talker, a C++ listener, AND a Rust heartbeat node combined into one bootable
//! `nano_ros_entry(BOARD zephyr LAUNCH …)` image. The Zephyr sibling of the threadx-linux /
//! FreeRTOS mixed entries.
//!
//! Unlike the C / C++ zephyr entries, a mixed entry must bundle the Rust node into ONE Rust
//! staticlib (the single-runtime invariant: one nros-rmw-cffi REGISTRY). The entry sets
//! `NROS_WS_RUST_NODE_DIRS` before find_package(Zephyr), so the nano-ros Zephyr module
//! synthesises + builds the `nros_ws_runtime` umbrella (nros-cpp + the node) and links THAT in
//! place of plain nros-cpp — the west-lane analog of the cmake/Corrosion archive swap. On
//! native_sim the umbrella is the host-std crate the mixed-native entry already proves.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the native_sim `zephyr.exe` runs the
//! `demo_bringup` talker, and a SEPARATE native C listener (`native_entry_robot2`, a
//! language-agnostic `/chatter` subscriber on the wire) receives it through a zenoh router on
//! the exact port baked into the fixture (17843).
//!
//! The fixture is built by the west lane; this test skips cleanly when `zephyr.exe` is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test mixed_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
    build_native_workspace_c_entry_robot2, build_zephyr_workspace_mixed_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the mixed zephyr workspace entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17843"`).
const ZEPHYR_ENTRY_PORT: u16 = 17843;

#[test]
fn mixed_zephyr_workspace_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = build_zephyr_workspace_mixed_entry().unwrap_or_else(|e| {
        nros_tests::skip!("mixed zephyr workspace entry not built (west): {e}")
    });
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    let router = ZenohRouter::start_on("127.0.0.1", ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {ZEPHYR_ENTRY_PORT}: {e}")
    });
    let observer_locator = router.locator();

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

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    let out = obs
        .wait_for_output_count("Received:", 3, Duration::from_secs(90))
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "native observer never received the mixed Zephyr native_sim entry's /chatter — \
                 the embedded mixed (C+C++/Rust) Zephyr LAUNCH-entry runtime delivery did not work"
            )
        });

    zephyr.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
