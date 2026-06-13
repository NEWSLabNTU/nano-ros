//! Phase 211.F — multi-host launch, end-to-end at RUNTIME (2 processes).
//!
//! `multihost_partition_bake` proves the per-host bake at the SOURCE level
//! (`nros codegen entry --host robotN` emits an entry registering only that
//! host's node). This test proves the other half: the per-host entries, when
//! BUILT and BOOTED as two separate processes (the multi-host topology),
//! actually exchange data across hosts.
//!
//! `multihost.launch.xml` places the talker on `robot1` and the listener on
//! `robot2`. The `native_entry_robot{1,2}` fixtures are
//! `nros::main!(launch = "demo_bringup:multihost.launch.xml", host = "robotN")`
//! — the macro's `host` filter keeps only that host's node, so `robot1` bakes
//! the talker and `robot2` the listener. We run both via the macro's env-gated
//! hosted spin and assert the `robot2` listener's subscription callback fires on
//! `robot1`'s publishes (`message_callbacks=N`, N ≥ 1) — cross-host delivery
//! through `zenohd`.
//!
//! Cross-process by construction: the two hosts are separate OS processes, so
//! the zenoh-pico in-process "write filter" limitation (see
//! `deployed_native_system_e2e`) does not apply.

use std::{process::Command, time::Duration};

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_rust_entry_robot1,
    build_native_workspace_rust_entry_robot2, require_zenohd, zenohd_unique,
};
use rstest::rstest;

/// The `robot1` (talker) + `robot2` (listener) per-host entries, booted as two
/// processes, deliver `/chatter` across hosts.
#[rstest]
fn multihost_two_process_delivers_across_hosts(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let robot1 = match build_native_workspace_rust_entry_robot1() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("robot1 per-host entry not built: {e}"),
    };
    let robot2 = match build_native_workspace_rust_entry_robot2() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!("robot2 per-host entry not built: {e}"),
    };
    let locator = zenohd_unique.locator();

    // robot2 (listener) first so its subscription is declared before robot1
    // starts publishing. Hosted spin counts subscription callbacks and prints
    // `message_callbacks=N` on exit.
    let mut r2 = Command::new(&robot2);
    r2.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "12000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10")
        .env("NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS", "1");
    let mut listener = ManagedProcess::spawn_command(r2, "robot2-listener").expect("spawn robot2");

    let mut r1 = Command::new(&robot1);
    r1.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "9000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut talker = ManagedProcess::spawn_command(r1, "robot1-talker").expect("spawn robot1");

    // robot2's hosted spin exits printing the callback count once its budget
    // elapses; wait for that line.
    let listener_out = listener
        .wait_for_output_pattern("hosted spin complete", Duration::from_secs(20))
        .expect("robot2 listener did not finish its hosted spin");

    talker.kill();
    listener.kill();

    // `message_callbacks=N` — the listener's subscription callback fired on the
    // talker's cross-host publishes. N ≥ 1 proves multi-host delivery.
    let got_delivery = listener_out
        .lines()
        .filter_map(|l| l.split("message_callbacks=").nth(1))
        .filter_map(|s| s.split_whitespace().next())
        .filter_map(|n| n.parse::<u32>().ok())
        .any(|n| n >= 1);
    assert!(
        got_delivery,
        "robot2 (listener) must receive robot1 (talker)'s /chatter across hosts \
         (expected `message_callbacks=N` with N>=1):\n{listener_out}"
    );
}
