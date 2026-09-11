//! A multi-node Rust entry shows one graph node per launch component in
//! `ros2 node list` — and the SAME set on every RMW (issue 1269).
//!
//! The image is `examples/workspaces/rust`'s default launch,
//! `demo_bringup:system.launch.xml`, a 2-node topology:
//!   `<node name="talker"/>` + `<node name="listener"/>`
//! built once per RMW from the same sources: `[image.native]` (zenoh, row
//! `workspace-rust-native`) and `[image.native_cyclonedds]` (row
//! `workspace-rust-native-cyclonedds`). Each case asserts [`EXPECTED_NODES`],
//! the one set every RMW must present, so a divergence between backends fails
//! a test rather than a user.
//!
//! The two backends announce nodes through unrelated mechanisms, which is why
//! each needs its own live case:
//!
//! * zenoh — one liveliness token per node (`ensure_node_liveliness` in the
//!   shared zenoh shim, phase-268 W2; the same gate the C++ proof in
//!   `cpp_multi_node_entry.rs` exercises);
//! * Cyclone — one `NodeEntitiesInfo` per node in the participant's
//!   `ros_discovery_info` sample (issue 1269; it used to publish ONE,
//!   session-named). The self-contained half of that claim is the backend's
//!   `graph_node_set` CTest, which reads the published sample back.
//!
//! XRCE publishes no node at all yet: `native-multinode-rust-xrce-CARVED`,
//! issue 1292.

use nros_tests::{
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, ZenohRouter,
        build_native_workspace_rust_cyclonedds_entry, build_native_workspace_rust_entry,
        is_rmw_zenoh_available, is_ros2_available, require_zenohd, ros2_node_list, zenohd_unique,
    },
    ros2::{require_ros2_cyclonedds, ros2_node_list_rmw_with_domain},
};
use rstest::rstest;
use std::{
    collections::BTreeSet,
    process::Command,
    time::{Duration, Instant},
};

/// The node set the image composes — the launch file's two `<node name=…/>`,
/// in the root namespace. Every RMW must present exactly this: the #104 / 1269
/// phantom `/node` (the SESSION's name, which names no component) is excluded
/// by construction, since equality admits nothing extra.
const EXPECTED_NODES: [&str; 2] = ["/listener", "/talker"];

/// The fully qualified node names a `ros2 node list` printed. Hidden nodes are
/// already filtered by the CLI (it needs `--all` to show them), so every
/// `/`-prefixed line is a node the graph attributes to someone.
fn node_set(listing: &str) -> BTreeSet<String> {
    listing
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

fn expected_set() -> BTreeSet<String> {
    EXPECTED_NODES.iter().map(|s| (*s).to_owned()).collect()
}

/// Poll until the listing shows exactly [`EXPECTED_NODES`], or the deadline.
/// Discovery is asynchronous on both backends, so one sample right after
/// start-up legitimately shows a partial graph.
fn poll_for_expected_set<F>(timeout: Duration, mut poll: F) -> String
where
    F: FnMut() -> String,
{
    let want = expected_set();
    let deadline = Instant::now() + timeout;
    let mut output = String::new();
    while Instant::now() < deadline {
        output = poll();
        if node_set(&output) == want {
            return output;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    output
}

/// The shared verdict, so the two cases cannot drift into asking different
/// questions: the set must EQUAL [`EXPECTED_NODES`].
fn assert_node_set(rmw: &str, listing: &str) {
    let got = node_set(listing);
    assert_eq!(
        got,
        expected_set(),
        "{rmw}: `ros2 node list` must show exactly the image's launch nodes {EXPECTED_NODES:?} \
         (issue 1269: the node set must not depend on the RMW). Full listing:\n{listing}"
    );
}

/// zenoh: `workspace-rust-native` `native_entry`, through a unique router.
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

    let node_list = poll_for_expected_set(Duration::from_secs(15), || {
        ros2_node_list(&locator, DEFAULT_ROS_DISTRO).unwrap_or_default()
    });
    deploy.kill();

    eprintln!("Node list (zenoh):\n{node_list}");
    assert_node_set("zenoh", &node_list);
    Ok(())
}

/// Cyclone: `workspace-rust-native-cyclonedds` `native_cyclonedds_entry`, on a
/// domain of its own and pinned to loopback on BOTH sides (issue 1137 — the
/// `ros2` side is pinned by its env string, ours by `apply_to_command`; half a
/// pin is no discovery, and reads as an empty graph).
#[test]
fn rust_multi_node_entry_per_node_graph_nodes_cyclonedds() -> nros_tests::TestResult<()> {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    let entry = match build_native_workspace_rust_cyclonedds_entry() {
        Ok(p) => p.to_path_buf(),
        Err(e) => nros_tests::skip!(
            "workspace-rust-native-cyclonedds native_cyclonedds_entry fixture not built: {e}"
        ),
    };

    // A domain of our own: Cyclone discovers by SPDP, so a shared domain would
    // let another test's nodes into this listing and break the equality below.
    let domain = nros_tests::unique_ros_domain_id();

    let mut cmd = Command::new(&entry);
    cmd.env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_DOMAIN_ID", domain.to_string())
        // The registered backend name, never an ambient lane token (AGENTS.md
        // "`NROS_RMW` footgun").
        .env("NROS_RMW", "cyclonedds")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    nros_tests::dds_isolation::apply_to_command(&mut cmd);
    let mut deploy = ManagedProcess::spawn_command(cmd, "rust-native-cyclonedds-entry")
        .expect("failed to start native_cyclonedds_entry");

    let node_list = poll_for_expected_set(Duration::from_secs(20), || {
        ros2_node_list_rmw_with_domain(DEFAULT_ROS_DISTRO, "rmw_cyclonedds_cpp", domain)
            .unwrap_or_default()
    });
    deploy.kill();

    eprintln!("Node list (cyclonedds):\n{node_list}");
    assert_node_set("cyclonedds", &node_list);
    Ok(())
}

// phase-329 W3 — bind this test to `interop::CELLS` (the pattern from
// xrce_ros2_interop). The coordinates below must equal what the list declares
// for `rust_multi_node_per_node_graph` — one per Runtime cell; drift turns this
// RED. Needs no fixtures — runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "rust_multi_node_per_node_graph",
        &[
            (Linux, Rust, Zenoh, EntryPubsub),
            (Linux, Rust, Cyclonedds, EntryPubsub),
        ],
    );
}
