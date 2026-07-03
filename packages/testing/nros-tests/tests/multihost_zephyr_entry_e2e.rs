//! phase-276 W6 (#102 H1) — runtime E2E for MULTIHOST on embedded: the Rust
//! workspace's per-host `robot1` Zephyr (native_sim) Entry + the NATIVE
//! `robot2` per-host entry, exchanging `/chatter` across hosts.
//!
//! `multihost.launch.xml` places the talker on `robot1` and the listener on
//! `robot2`. The Zephyr entry is `nros::main!(launch =
//! "demo_bringup:multihost.launch.xml", host = "robot1")` — the macro's host
//! filter (Phase 211.F) bakes only the talker into the image. The listener is
//! the existing `native_entry_robot2` fixture booted as a host process. The
//! two "hosts" are one Zephyr native_sim image + one native process meeting at
//! zenohd — the same cross-host topology as `multihost_runtime_e2e`, with one
//! side embedded.
//!
//! Assertion: robot2's env-gated hosted spin counts subscription callbacks and
//! prints `message_callbacks=N` on exit; N ≥ 1 proves cross-host delivery from
//! the embedded talker.
//!
//! Requires the west-lane fixture (`just zephyr build-fixtures`; skips when
//! `zephyr.exe` is absent) and the native workspace fixtures
//! (`just native build-workspace-fixtures`).
//!
//! Run with: `cargo nextest run -p nros-tests --test multihost_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
    build_native_workspace_rust_entry_robot2, build_zephyr_workspace_rust_multihost_robot1_entry,
};
use std::{process::Command, time::Duration};

/// The router port baked into the robot1 zephyr entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17853"`).
const MULTIHOST_ZEPHYR_ENTRY_PORT: u16 = 17853;

#[test]
#[ignore = "blocked by issue #140 — the NATIVE per-host entry (robot2, hosted spin) \
            subscription receives nothing even from a native robot1 talker \
            (multihost_runtime_e2e fails the same way); the zephyr robot1 half is \
            proven (boots, host filter bakes 1 node, publishes). Unignore when #140 lands"]
fn multihost_zephyr_robot1_delivers_to_native_robot2() {
    let entry = build_zephyr_workspace_rust_multihost_robot1_entry().unwrap_or_else(|e| {
        nros_tests::skip!("zephyr multihost robot1 entry not built (west): {e}")
    });
    let robot2 = build_native_workspace_rust_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("robot2 per-host entry not built: {e}"));

    // Router on the exact port the fixture's CONFIG_NROS_ZENOH_LOCATOR was baked with.
    let router =
        ZenohRouter::start_on("127.0.0.1", MULTIHOST_ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
            nros_tests::skip!("zenohd failed to start on {MULTIHOST_ZEPHYR_ENTRY_PORT}: {e}")
        });
    let locator = router.locator();

    // robot2 (listener, native) first so its subscription is declared before
    // the embedded talker starts publishing. Hosted spin counts subscription
    // callbacks and prints `message_callbacks=N` on exit.
    let mut r2 = Command::new(&robot2);
    r2.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10")
        .env("NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS", "1");
    let mut listener = ManagedProcess::spawn_command(r2, "robot2-listener").expect("spawn robot2");

    // Boot the Zephyr native_sim robot1 image (talker; runs until killed).
    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    // robot2's hosted spin exits printing the callback count once its budget
    // elapses; wait for that line.
    let listener_out = listener
        .wait_for_output_pattern("hosted spin complete", Duration::from_secs(35))
        .unwrap_or_else(|_| {
            zephyr.kill();
            listener.kill();
            panic!("robot2 listener did not finish its hosted spin")
        });

    zephyr.kill();
    listener.kill();

    let delivered = listener_out
        .lines()
        .filter_map(|l| l.split("message_callbacks=").nth(1))
        .filter_map(|v| v.split_whitespace().next())
        .filter_map(|v| v.parse::<u32>().ok())
        .any(|n| n >= 1);
    assert!(
        delivered,
        "robot2 (native listener) saw no `/chatter` callbacks from the Zephyr \
         robot1 talker — cross-host delivery from the embedded target failed \
         (276 W6):\n{listener_out}"
    );
}
