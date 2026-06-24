//! phase-263 Track C — runtime E2E for the C++ workspace multihost demo (C++ parity
//! with the Rust `multihost_runtime_e2e` and the C `c_multihost_e2e`).
//!
//! `examples/workspaces/cpp`'s `multihost.launch.xml` places the talker on `robot1` and
//! the listener on `robot2`. The CMake `nano_ros_entry(HOST <id> …)` passthrough bakes
//! `native_entry_robot1` talker-only and `native_entry_robot2` listener-only. Booting
//! both as two processes and observing `robot2` print `Received:` proves cross-host
//! delivery through the host-partitioned C++ entries.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_multihost_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_workspace_cpp_entry_robot1,
    build_native_workspace_cpp_entry_robot2, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

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

/// Track C — the host-partitioned C++ entries deliver across hosts: `robot1` (talker
/// only) publishes `/chatter`, `robot2` (listener only) receives it.
#[rstest]
fn cpp_multihost_delivers_across_hosts(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    let robot1 = build_native_workspace_cpp_entry_robot1()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ robot1 entry fixture not built: {e}"));
    let robot2 = build_native_workspace_cpp_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("C++ robot2 entry fixture not built: {e}"));
    let locator = zenohd_unique.locator();

    // The C++ listener prints no ready marker (only `Received:`), so settle briefly to
    // let its subscription come up before robot1 publishes.
    let mut r2 = spawn_entry(robot2, "robot2-listener", &locator, 12000);
    std::thread::sleep(Duration::from_millis(1500));
    let mut r1 = spawn_entry(robot1, "robot1-talker", &locator, 12000);

    let out = r2
        .wait_for_output_count("Received:", 3, Duration::from_secs(18))
        .unwrap_or_else(|_| {
            r1.kill();
            r2.kill();
            panic!(
                "robot2 (listener-only host entry) never received robot1's /chatter — \
                 the C++ multihost host-partition delivery did not work"
            )
        });

    r1.kill();
    r2.kill();

    let n = nros_tests::count_pattern(&out, "Received:");
    assert!(
        n >= 3,
        "expected ≥3 cross-host deliveries on robot2, got {n}"
    );
}
