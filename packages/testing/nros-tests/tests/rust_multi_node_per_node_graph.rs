//! phase-268 W3 — e2e proof (Rust path) that a multi-node Rust entry shows
//! one graph node per launch component in `ros2 node list`, named from the
//! launch, with the #104 primary `/node` gated off.
//!
//! The `workspace-rust-native` fixture bakes `native_entry` from
//! `examples/workspaces/rust`, whose `nros::main!(launch =
//! "demo_bringup:system.launch.xml")` resolves a 2-node topology:
//!   `<node name="talker"/>` + `<node name="listener"/>`
//!
//! The W2 gate lives in the shared zenoh shim
//! (`ensure_node_liveliness` in `zpico/nros-rmw-zenoh/src/shim/session.rs`),
//! so this test exercises the same code path as the C++ proof in
//! `cpp_multi_node_entry.rs` — proving the gate is language-agnostic.

use nros_tests::fixtures::{
    DEFAULT_ROS_DISTRO, ManagedProcess, ZenohRouter, build_native_workspace_rust_entry,
    is_rmw_zenoh_available, is_ros2_available, require_zenohd, ros2_node_list, zenohd_unique,
};
use rstest::rstest;
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn poll_until_contains<F>(timeout: Duration, marker: &str, mut poll: F) -> String
where
    F: FnMut() -> String,
{
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    while Instant::now() < deadline {
        output = poll();
        if output.contains(marker) {
            return output;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    output
}

/// W3 e2e proof (Rust path): `workspace-rust-native` `native_entry` bakes
/// `demo_bringup:system.launch.xml` (talker + listener) and exposes both as
/// distinct graph nodes in `ros2 node list`, with the #104 primary `/node`
/// gated off by W2 `ensure_node_liveliness`.
#[rstest]
fn rust_multi_node_entry_per_node_graph_nodes(
    zenohd_unique: ZenohRouter,
) -> nros_tests::TestResult<()> {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !is_ros2_available() {
        nros_tests::skip!("ROS 2 not found");
    }
    if !is_rmw_zenoh_available() {
        nros_tests::skip!("rmw_zenoh_cpp not found");
    }

    let entry = match build_native_workspace_rust_entry() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            nros_tests::skip!("workspace-rust-native native_entry fixture not built: {e}")
        }
    };

    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(&entry);
    cmd.env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut deploy = ManagedProcess::spawn_command(cmd, "rust-native-entry")
        .expect("failed to start native_entry");

    let node_list = poll_until_contains(Duration::from_secs(15), "listener", || {
        ros2_node_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });
    deploy.kill();

    eprintln!("Node list:\n{node_list}");
    // W1: both launch-named graph nodes must appear.
    assert!(
        node_list.contains("talker"),
        "Expected '/talker' in ros2 node list (W1 Rust launch name), got:\n{node_list}"
    );
    assert!(
        node_list.contains("listener"),
        "Expected '/listener' in ros2 node list (W1 Rust launch name), got:\n{node_list}"
    );
    // W2 gate: the #104 primary /node token must be absent when per-node tokens
    // with different names are active. Match the whole trimmed line so prefixes
    // like /node_factory or /node0 do not cause a false failure.
    assert!(
        !node_list.lines().any(|l| l.trim() == "/node"),
        "W2 gate broken: bare '/node' in ros2 node list (issue #104):\n{node_list}"
    );
    Ok(())
}
