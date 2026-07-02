//! phase-263 C2d — runtime E2E for the Zephyr (native_sim) C workspace EMBEDDED entry:
//! `nano_ros_entry(BOARD zephyr LAUNCH …)`, the C/C++ sibling of the C2a/C2b/C2c entries on
//! the west build lane.
//!
//! Zephyr's build model differs (west, `find_package(Zephyr)` → `app` target, not
//! add_executable). `nano_ros_entry`'s zephyr branch is the C/C++ analog of
//! zephyr-lang-rust's `rust_cargo_application()`: it puts the generated entry TU (an
//! `int main(void)` driving `ZephyrBoard::run_components`) into `app` (whole-archived →
//! strong main) and links the node component libs in. The connect locator threads in via the
//! compile-time `CONFIG_NROS_ZENOH_LOCATOR` Kconfig the west build bakes.
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the native_sim `zephyr.exe` runs the
//! `demo_bringup` talker, and a SEPARATE native C listener (`native_entry_robot2`, a
//! language-agnostic `/chatter` subscriber on the wire) receives it through a zenoh router.
//! native_sim's NSOS shim forwards to host sockets, so a `tcp/127.0.0.1` locator is reachable
//! with no bridge / root. The router uses the exact port baked into the fixture.
//!
//! The fixture is built by the west lane (`just zephyr build-fixtures`); this test skips
//! cleanly when `zephyr.exe` is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
    build_native_workspace_c_entry_robot2, build_zephyr_workspace_c_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C zephyr workspace entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17831"`).
const ZEPHYR_ENTRY_PORT: u16 = 17831;

#[test]
fn zephyr_c_workspace_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = build_zephyr_workspace_c_entry()
        .unwrap_or_else(|e| nros_tests::skip!("zephyr C workspace entry not built (west): {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router = ZenohRouter::start_on("127.0.0.1", ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {ZEPHYR_ENTRY_PORT}: {e}")
    });
    let observer_locator = router.locator();

    // External observer (native listener) first, so its subscription is live before the
    // native_sim talker publishes.
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

    // Boot the Zephyr native_sim image (runs until killed — no bounded spin on the embedded
    // target). It dials the baked tcp/127.0.0.1 locator through NSOS host sockets.
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // The observer prints `Received: <n>` per delivered message — 3 confirms the Zephyr
    // guest's talker reached a separate process through the router. native_sim publishes a
    // few per second; allow a generous window.
    let out = obs
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "native observer never received the Zephyr native_sim entry's /chatter — \
                 the embedded Zephyr LAUNCH-entry runtime delivery did not work"
            )
        });

    zephyr.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
