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
//! * XRCE — the same `ros_discovery_info` sample, written by the CLIENT
//!   through the Agent (issue 1292). The client cannot read the GUIDs the
//!   Agent gives its endpoints, so it learns the participant's GUID prefix from
//!   a self-addressed request and predicts each endpoint's key from the Agent's
//!   numbering (`nros-rmw-xrce/src/graph.c`). That is why the XRCE case also
//!   asks `ros2 node info`: a right node NAME with wrong GIDs still lists the
//!   node and attributes none of its endpoints.

use nros_tests::{
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, RequireFixture, XrceAgent, ZenohRouter,
        build_native_workspace_rust_cyclonedds_entry, build_native_workspace_rust_entry,
        build_native_workspace_rust_xrce_entry, is_rmw_zenoh_available, is_ros2_available,
        require_ros2_dds, require_xrce_agent, require_zenohd, ros2_node_list, zenohd_unique,
    },
    ros2::{
        require_ros2_cyclonedds, ros2_node_info_rmw_with_domain, ros2_node_list_rmw_with_domain,
    },
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

    let entry = build_native_workspace_rust_entry()
        .require("workspace-rust-native native_entry")
        .to_path_buf();

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

    let entry = build_native_workspace_rust_cyclonedds_entry()
        .require("workspace-rust-native-cyclonedds native_cyclonedds_entry")
        .to_path_buf();

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

/// What `ros2 node info` must attribute to each launch node — the talker
/// publishes `/chatter`, the listener subscribes to it. An XRCE sample whose
/// GIDs were wrong would still name both nodes and list neither endpoint.
const EXPECTED_ENDPOINTS: [(&str, &str, &str); 2] = [
    ("/talker", "Publishers:", "/chatter"),
    ("/listener", "Subscribers:", "/chatter"),
];

/// The topics `ros2 node info` lists under `section` ("Publishers:", …), in
/// the order printed. The section runs until the next line ending in `:`.
fn node_info_section(info: &str, section: &str) -> Vec<String> {
    let mut lines = info.lines().map(str::trim);
    if !lines.any(|l| l == section) {
        return Vec::new();
    }
    lines
        .take_while(|l| !l.ends_with(':'))
        .filter(|l| l.starts_with('/'))
        .map(|l| l.split(':').next().unwrap_or(l).trim().to_owned())
        .collect()
}

/// XRCE: `workspace-rust-native-xrce` `native_xrce_entry`, through its own
/// Agent, on a domain of its own. The image speaks XRCE and the AGENT is the
/// DDS participant, so the Agent is what gets pinned to loopback
/// (`XrceAgent::start_unique` applies the issue-1009 profile), opposite a
/// `rmw_fastrtps_cpp` peer pinned by its env string.
#[test]
fn rust_multi_node_entry_per_node_graph_nodes_xrce() -> nros_tests::TestResult<()> {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_ros2_dds() {
        nros_tests::skip!("ROS 2 + rmw_fastrtps_cpp not available");
    }

    let entry = build_native_workspace_rust_xrce_entry()
        .require("workspace-rust-native-xrce native_xrce_entry")
        .to_path_buf();

    let agent = XrceAgent::start_unique().expect("failed to start the XRCE Agent");
    let addr = agent.addr();
    let domain = nros_tests::unique_ros_domain_id();

    let mut cmd = Command::new(&entry);
    cmd.env("NROS_LOCATOR", &addr)
        .env("XRCE_AGENT_ADDR", &addr)
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_DOMAIN_ID", domain.to_string())
        .env("NROS_RMW", "xrce")
        .env("NROS_ENTRY_SPIN_MS", "25000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut deploy = ManagedProcess::spawn_command(cmd, "rust-native-xrce-entry")
        .expect("failed to start native_xrce_entry");

    let node_list = poll_for_expected_set(Duration::from_secs(20), || {
        ros2_node_list_rmw_with_domain(DEFAULT_ROS_DISTRO, "rmw_fastrtps_cpp", domain)
            .unwrap_or_default()
    });
    eprintln!("Node list (xrce):\n{node_list}");

    // Asked only once the names are there: before that, an empty section is
    // discovery still running, not a wrong GID.
    let mut infos = Vec::new();
    if node_set(&node_list) == expected_set() {
        for (node, section, topic) in EXPECTED_ENDPOINTS {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut info = String::new();
            while Instant::now() < deadline {
                info = ros2_node_info_rmw_with_domain(
                    DEFAULT_ROS_DISTRO,
                    "rmw_fastrtps_cpp",
                    domain,
                    node,
                )
                .unwrap_or_default();
                if node_info_section(&info, section).iter().any(|t| t == topic) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
            eprintln!("Node info {node} (xrce):\n{info}");
            infos.push((node, section, topic, info));
        }
    }
    deploy.kill();

    assert_node_set("xrce", &node_list);
    for (node, section, topic, info) in &infos {
        assert!(
            node_info_section(info, section).iter().any(|t| t == topic),
            "xrce: `ros2 node info {node}` must list {topic} under {section} — the node is \
             named by ros_discovery_info, but its endpoint is attributed to it only if the \
             GID the client predicted is the one the Agent gave (issue 1292). Full output:\n{info}"
        );
    }
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
            (Linux, Rust, Xrce, EntryPubsub),
        ],
    );
}
