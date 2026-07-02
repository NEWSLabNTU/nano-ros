//! phase-263 Track C — runtime E2E for the C workspace multihost demo (C parity with
//! the Rust `multihost_runtime_e2e`).
//!
//! `examples/workspaces/c`'s `multihost.launch.xml` places the talker on `robot1` and
//! the listener on `robot2` (`<node machine="…">`). The CMake `nano_ros_entry(HOST <id>
//! …)` passthrough shells `nros codegen entry --host <id>`, so `native_entry_robot1`
//! bakes talker-only and `native_entry_robot2` listener-only — the same per-host bake
//! the Rust workspace gets from `nros::main!(host = …)`. Booting both as two processes
//! and observing the `robot2` listener print `Received:` proves cross-host delivery
//! through the host-partitioned C entries.
//!
//! Run with: `cargo nextest run -p nros-tests --test c_multihost_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_c_entry_robot1,
    build_native_workspace_c_entry_robot2, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// Spawn a per-host C entry binary on `locator`, spinning for `spin_ms`.
fn spawn_entry(
    entry: std::path::PathBuf,
    label: &'static str,
    locator: &str,
    spin_ms: u32,
) -> ManagedProcess {
    let mut cmd = Command::new(entry);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string());
    ManagedProcess::spawn_command(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"))
}

/// Track C — the host-partitioned C entries deliver across hosts: `robot1` (talker
/// only) publishes `/chatter`, `robot2` (listener only) receives it.
#[rstest]
fn c_multihost_delivers_across_hosts(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let robot1 = build_native_workspace_c_entry_robot1()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C robot1 entry fixture not built: {e}"));
    let robot2 = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C robot2 entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    // Listener (robot2) first, so its subscription is live before robot1 publishes.
    let mut r2 = spawn_entry(robot2, "robot2-listener", &locator, 12000);
    r2.wait_for_output_pattern("Waiting for messages", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            r2.kill();
            panic!("robot2 listener never became ready")
        });
    let mut r1 = spawn_entry(robot1, "robot1-talker", &locator, 12000);

    // robot2 prints `Received: <n>` per delivered message — 3 confirms cross-host
    // delivery through the host-partitioned entries.
    let out = r2
        .wait_for_output_count(
            nros_tests::output::INT32_LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(18),
        )
        .unwrap_or_else(|_| {
            r1.kill();
            r2.kill();
            panic!(
                "robot2 (listener-only host entry) never received robot1's /chatter — \
                 the C multihost host-partition delivery did not work"
            )
        });

    r1.kill();
    r2.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::INT32_LISTENER_LOG_PREFIX);
    assert!(
        n >= 3,
        "expected ≥3 cross-host deliveries on robot2, got {n}"
    );
}
