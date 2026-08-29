//! phase-381 acceptance — READ the ROS graph a stock ROS 2 node is in.
//!
//! This is the only test in the phase whose subject is DISCOVERY rather than
//! delivery, and it exists because of what happened without it.
//!
//! Phase-381 shipped twelve `rmw` graph slots: produced, reachable from Rust, C
//! and C++, with mutation-tested unit coverage and a clean `check-api-parity`.
//! Every one of those checks tested our code against our own builders, our own
//! parser and our own vtable — and the feature did not work. Issue 0903 was
//! THREE stacked defects (a drain restarting on the first reply rather than the
//! finished sweep; a runtime that dispatched exactly one of eleven graph
//! methods; a `collect` flag set AFTER the query went on the wire, so every
//! reply took the single-response path), and no unit test could see any of
//! them, because each one only manifests against a real peer.
//!
//! So the assertion here is deliberately the one thing a self-contained test
//! cannot make: a nano-ros node enumerates a live `rmw_zenoh_cpp` peer, and
//! stock `ros2 node list` enumerates ours.
//!
//! Interop cell: `native-graph-rust-zenoh-r2n` (`interop::CELLS`).

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures, interop,
    ros2::{DEFAULT_ROS_DISTRO, Ros2Process, require_ros2, ros2_node_list},
};

/// The nano-ros side sees a stock ROS 2 node.
///
/// Polls rather than sampling once: the graph slots report what has ALREADY
/// arrived and never block, so a single call after startup legitimately returns
/// a partial graph. Written as one comparison this would be flaky by
/// construction — Design note 3 of the phase doc.
#[test]
fn nano_ros_enumerates_a_stock_ros2_node() {
    // The coordinate tripwire: this test is bound to its `interop::CELLS` row,
    // so a cell added without a test (or a test that drifts off its cell) is a
    // failure rather than silent non-coverage.
    interop::assert_test_bound(
        "graph_interop",
        &[(
            nros_tests::matrix::PlatformId::Linux,
            nros_tests::matrix::Lang::Rust,
            nros_tests::matrix::Rmw::Zenoh,
            nros_tests::matrix::Workload::Graph,
        )],
    );

    if !require_ros2() {
        nros_tests::skip!("ROS 2 + rmw_zenoh_cpp not available");
    }
    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let _talker = Ros2Process::demo_nodes_cpp_talker(&locator, DEFAULT_ROS_DISTRO)
        .expect("start the stock talker");

    // The probe polls to convergence and exits non-zero unless it sees the
    // node named here — so an EMPTY graph is a failure, not a quiet pass. That
    // distinction is the whole point: issue 0903 presented as "zero topics",
    // which is indistinguishable from "no topics exist" unless something
    // asserts a peer must be visible.
    let probe = fixtures::build_graph_probe().expect("prebuilt graph-probe");
    let out = Command::new(probe)
        .env("NROS_LOCATOR", &locator)
        .env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_TIMEOUT_MS", "20000")
        .output()
        .expect("run graph-probe");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // The probe exits non-zero when it does not see the expected peer, so this
    // is asserted on the STATUS as well as the marker — a probe that printed
    // the marker but failed would be a different bug, and one worth catching.
    assert!(
        out.status.success(),
        "graph-probe must exit 0 once it sees the talker; got {:?}\n{output}",
        out.status.code()
    );
    let _ = Duration::from_secs(0);

    assert!(
        output.contains(nros_tests::output::GRAPH_PROBE_SAW),
        "the nano-ros node must ENUMERATE the stock talker; probe said:\n{output}"
    );

    // And the reverse direction, which is what `ros2 node list` answers. Our
    // node was visible in the graph long before it could read one — that
    // asymmetry is what issue 0791 filed — so asserting only our side would
    // pass on a build that had regressed to write-only.
    let listed = ros2_node_list(&locator, DEFAULT_ROS_DISTRO).expect("ros2 node list");
    assert!(
        listed.contains("talker"),
        "ros2 node list must see the stock talker (sanity: the graph is live):\n{listed}"
    );
}
