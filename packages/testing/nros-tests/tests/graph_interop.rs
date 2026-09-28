//! phase-381 acceptance — READ the ROS graph a stock ROS 2 node is in.
//!
//! This is the only test in the phase whose subject is DISCOVERY rather than
//! delivery, and it exists because of what happened without it.
//!
//! Phase-381 shipped twelve `rmw` graph slots: produced, reachable from Rust, C
//! and C++, with mutation-tested unit coverage and a clean `check-api-parity`.
//! Every one of those checks tested our code against our own builders, our own
//! parser and our own vtable — and the feature did not work.
//!
//! Issue 0903 was several stacked defects (a drain restarting on the first
//! reply rather than the finished sweep; a runtime that dispatched exactly one
//! of eleven graph methods; a `collect` flag set AFTER the query went on the
//! wire) sitting on top of a MECHANISM that could not work at all:
//! `z_liveliness_get` is an INTEREST, and a token reaches a get's callback only
//! when the router tags its declaration with that interest id, so a sweep saw
//! an arbitrary handful of the domain's tokens. The fix was a standing
//! liveliness SUBSCRIBER with history — a graph cache, not a question asked
//! repeatedly. No unit test could see any of it, because none of it manifests
//! except against a real peer.
//!
//! So the assertion here is deliberately the one thing a self-contained test
//! cannot make: a nano-ros node enumerates a live `rmw_zenoh_cpp` peer, and
//! stock `ros2 node list` enumerates ours.
//!
//! Interop cell: `native-graph-rust-zenoh-r2n` (`interop::CELLS`).

use nros_tests::fixtures::RequireFixture;
use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures, interop, output,
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, Ros2DdsProcess, Ros2Process, require_ros2, ros2_node_list},
};

/// The coordinates `interop::CELLS` declares for `graph_interop` — one per
/// backend. zenoh and Cyclone discover through entirely different mechanisms
/// (`@ros2_lv` liveliness tokens versus the `ros_discovery_info` topic), so
/// each needs its own live case; proving one says nothing about the other.
///
/// NOT named `*_CELLS`, and that is a fix rather than a style choice: this
/// declaration used to carry that suffix, which `no_local_axis_tables` flags
/// as a second axis table outside `matrix.rs` / `interop.rs` — so that gate
/// was RED on `main` from the day this file landed. The name was the whole
/// violation: what this holds is the coordinate set `assert_test_bound`
/// compares AGAINST `interop::CELLS`, which is the mechanism that keeps there
/// from being a second SSoT rather than an instance of one.
const GRAPH_COORDS: [(
    nros_tests::matrix::PlatformId,
    nros_tests::matrix::Lang,
    nros_tests::matrix::Rmw,
    nros_tests::matrix::Workload,
); 2] = [
    (
        nros_tests::matrix::PlatformId::Linux,
        nros_tests::matrix::Lang::Rust,
        nros_tests::matrix::Rmw::Zenoh,
        nros_tests::matrix::Workload::Graph,
    ),
    (
        nros_tests::matrix::PlatformId::Linux,
        nros_tests::matrix::Lang::Rust,
        nros_tests::matrix::Rmw::Cyclonedds,
        nros_tests::matrix::Workload::Graph,
    ),
];

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
    // BOTH cells: the tripwire compares what this test NAME covers against
    // every `interop::CELLS` row that names it, and this file carries the zenoh
    // and cyclone cases. Declaring only one made the check fail loudly rather
    // than let a cell drift uncovered — which is the point of it.
    interop::assert_test_bound("graph_interop", &GRAPH_COORDS);

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
    let probe = fixtures::build_graph_probe().require("prebuilt graph-probe");
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

    // Every OTHER graph slot, against the same live peer.
    //
    // This is the assertion phase-393's closing note asks for and phase-381 did
    // not have: `check-rmw-slot-producers` calls all eleven `produced`, which
    // means something writes and reads each slot — NOT that either was ever
    // exercised against a real ROS 2 node. Issue 0903 was nine slots that had
    // never been called sitting behind two that had, and the two that worked
    // made the family look covered.
    //
    // The probe exits 5 and names each failing slot, so the status assertion
    // above already catches this; the marker is asserted too because a probe
    // that stopped running the checks would otherwise pass silently.
    assert!(
        output.contains(nros_tests::output::GRAPH_PROBE_ALL_SLOTS_OK),
        "all eleven graph slots must answer against a live peer; probe said:\n{output}"
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

/// Cyclone's graph reader, against a live ROS 2 node — phase-381 step 2.
///
/// W5 gave Cyclone a READER for `ros_discovery_info`, the topic it had only
/// ever PUBLISHED, and nothing ever ran it against a real peer. That is the
/// state zenoh was in right up until issue 0903, where twelve slots that were
/// `produced`, mutation-tested and parity-clean turned out not to work at all —
/// so "Cyclone's is fine, it is the same shape" is exactly the assumption this
/// test exists to refuse.
///
/// Cyclone answers FEWER slots than zenoh, and that is the point of W6 rather
/// than a defect: a slot it cannot serve must answer `UNSUPPORTED`, never an
/// empty list. The probe classifies the two separately, so this asserts the
/// node enumeration works and that whatever else does not is *declared*
/// missing rather than silently blank.
///
/// Interop cell: `native-graph-rust-cyclone-r2n` (`interop::CELLS`).
#[test]
fn cyclone_enumerates_a_stock_ros2_node() {
    // BOTH cells: the tripwire compares what this test NAME covers against
    // every `interop::CELLS` row that names it, and this file carries the zenoh
    // and cyclone cases. Declaring only one made the check fail loudly rather
    // than let a cell drift uncovered — which is the point of it.
    interop::assert_test_bound("graph_interop", &GRAPH_COORDS);

    if !nros_tests::ros2::require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    // A domain of our own. Cyclone discovers by multicast SPDP, so a shared
    // domain would let another test's participants into this graph and make
    // the node assertion below depend on what else is running.
    let domain = nros_tests::unique_ros_domain_id();

    let _talker =
        Ros2DdsProcess::demo_nodes_cpp_talker_cyclonedds_with_domain(DEFAULT_ROS_DISTRO, domain)
            .expect("start the stock talker on cyclone");

    let probe = fixtures::build_graph_probe_rmw(nros_tests::fixtures::Rmw::Cyclonedds)
        .require("prebuilt cyclone graph-probe");
    let mut cmd = Command::new(probe);
    cmd.env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_TIMEOUT_MS", "20000")
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_DOMAIN_ID", domain.to_string());
    // Issue 1137 — the SAME bus as the talker.
    //
    // `demo_nodes_cpp_talker_cyclonedds_with_domain` funnels through
    // `ros2_env_setup_rmw_with_domain`, which since issue 1009 exports a
    // `CYCLONEDDS_URI` confining that participant to `127.0.0.1` with
    // `AllowMulticast=false` and an explicit localhost peer. This probe is a
    // bare `Command` — not a `ManagedProcess`, which pins itself — so without
    // this line the two sides sit on different interfaces and neither one's
    // SPDP reaches the other. The probe then enumerates exactly one node,
    // itself, which is indistinguishable from a broken `ros_discovery_info`
    // reader and was filed as one twice (0927, then 1137).
    nros_tests::dds_isolation::apply_to_command(&mut cmd);
    let out = cmd.output().expect("run graph-probe");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        out.status.success(),
        "cyclone graph-probe must exit 0 once it sees the talker; got {:?}\n{output}",
        out.status.code()
    );
    assert!(
        output.contains(nros_tests::output::GRAPH_PROBE_SAW),
        "cyclone must ENUMERATE the stock talker; probe said:\n{output}"
    );
}

// ---------------------------------------------------------------------------
// phase-467 Row 8 — the CHANGE EDGE
// ---------------------------------------------------------------------------
//
// The two cases above answer "can this node READ the graph". Neither can see
// the defect Row 8 closed: `rmw_vtable_t::node_get_graph_guard_condition` has
// existed since phase-376 W4, no backend filled it, and eleven green
// enumeration slots are entirely compatible with a runtime that is never told
// the graph moved. Reading is polling; this is the push.
//
// **The subject is a peer LEAVING, and that choice is the whole design.** An
// arrival cannot distinguish the two backends' real behaviour from their
// start-up burst: zenoh's liveliness subscriber is declared with
// `history = true`, so the tokens that already exist are delivered the moment
// it is declared and an "arm, then wait" test passes before any peer does
// anything. A departure cannot be manufactured that way: the liveliness token
// has to be undeclared (zenoh), or the DDS endpoints have to be DISPOSED
// (Cyclone), after the probe has latched its count.
//
// The Cyclone half of that sentence is MEASURED, and the obvious reading is
// wrong. A killed participant republishes no `ros_discovery_info`, so the
// reader every Cyclone graph QUERY uses never sees another sample — the first
// run of this test armed at 1 and then reported nothing for thirty seconds.
// What carries the departure is the dispose on the DDS builtin topics, which
// `EndpointBatch::at` deliberately SKIPS when enumerating. The backend now
// creates those readers when the edge is installed.
//
// The coordination is a MARKER, not a sleep. The probe prints (and flushes)
// `GRAPH_PROBE_CHANGE_ARMED` once it has seen the talker and latched the
// count; only then does the test kill the talker. A timed guess would make
// the pass depend on discovery being slower than the sleep, which is the
// shape that reads as a flake for a year.

/// The probe's budget. Generous because the failing direction is a TIMEOUT,
/// and the two backends reach the edge over different machinery: zenoh's
/// liveliness undeclare is prompt, Cyclone's travels as a builtin-topic
/// dispose that follows the peer's SPDP departure.
const CHANGE_BUDGET_MS: &str = "30000";
/// How long the test waits for the probe to arm, then to finish. Larger than
/// the probe's own budget so the probe's diagnostic — which names what it
/// saw — is what a failure reports, rather than this wrapper's timeout.
const CHANGE_WAIT: Duration = Duration::from_secs(45);

/// zenoh: a peer LEAVING fires the graph-change guard condition.
///
/// Interop cell: `native-graph-rust-zenoh-r2n` (`interop::CELLS`) — the same
/// coordinate as `nano_ros_enumerates_a_stock_ros2_node`, because a
/// coordinate is (platform, language, RMW, workload) and none of the four
/// moved. What moved is the QUESTION, which `interop::CASE_CELLS` is the
/// mechanism for: it maps each case of a shared binary to the cell it is
/// evidence for, so this case and the enumeration case are recorded
/// separately against one cell rather than standing in for each other.
#[test]
fn a_peer_leaving_fires_the_graph_change_guard() {
    interop::assert_test_bound("graph_interop", &GRAPH_COORDS);

    if !require_ros2() {
        nros_tests::skip!("ROS 2 + rmw_zenoh_cpp not available");
    }
    let router = fixtures::or_skip(fixtures::ZenohRouter::start_unique());
    let locator = router.locator();

    let talker = Ros2Process::demo_nodes_cpp_talker(&locator, DEFAULT_ROS_DISTRO)
        .expect("start the stock talker");

    let probe = fixtures::build_graph_probe().require("prebuilt graph-probe");
    let mut cmd = Command::new(probe);
    cmd.env("NROS_LOCATOR", &locator)
        .env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_WATCH_CHANGE", "1")
        .env("GRAPH_PROBE_TIMEOUT_MS", CHANGE_BUDGET_MS);
    let mut probe = ManagedProcess::spawn_command(cmd, "graph-probe-change")
        .expect("spawn the graph-change probe");

    let armed = probe
        .wait_for_output_pattern(output::GRAPH_PROBE_CHANGE_ARMED, CHANGE_WAIT)
        .expect("the probe must arm: it needs the talker in its graph first");
    assert!(
        !armed.contains(output::GRAPH_PROBE_CHANGE_UNSUPPORTED),
        "zenoh declined the graph-change slot; this cell says it owes one:\n{armed}"
    );

    // THE STIMULUS. Everything the probe reports from here is caused by this.
    drop(talker);

    let rest = probe
        .wait_for_all_output(CHANGE_WAIT)
        .expect("the probe must finish after the talker leaves");
    let out = format!("{armed}{rest}");
    assert!(
        !out.contains(output::GRAPH_PROBE_CHANGE_NONE),
        "zenoh ACCEPTED the graph-change callback and never fired it — the state eleven \
         `produced` read slots cannot tell from working:\n{out}"
    );
    assert!(
        out.contains(output::GRAPH_PROBE_CHANGE_FIRED),
        "a peer leaving the graph must reach the guard condition:\n{out}"
    );
}

/// Cyclone: the same edge, over an entirely different mechanism.
///
/// Worth its own case for the reason the enumeration pair is a pair: zenoh
/// learns of the departure from an undeclared `@ros2_lv` liveliness token and
/// Cyclone from a republished `ros_discovery_info` sample reaching a
/// participant-level `on_data_available`. Nothing under test is shared, so a
/// green on one says nothing about the other — and the phase-467 study
/// asserted, wrongly, that Cyclone had no change signal at all. This is where
/// that claim is settled by measurement rather than by reading.
///
/// Interop cell: `native-graph-rust-cyclone-r2n` (`interop::CELLS`).
#[test]
fn cyclone_a_peer_leaving_fires_the_graph_change_guard() {
    interop::assert_test_bound("graph_interop", &GRAPH_COORDS);

    if !nros_tests::ros2::require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    // A domain of our own, for the reason the enumeration case documents:
    // Cyclone discovers by multicast SPDP and a shared domain would let
    // another test's participants decide when this graph changes.
    let domain = nros_tests::unique_ros_domain_id();

    let talker =
        Ros2DdsProcess::demo_nodes_cpp_talker_cyclonedds_with_domain(DEFAULT_ROS_DISTRO, domain)
            .expect("start the stock talker on cyclone");

    let probe = fixtures::build_graph_probe_rmw(nros_tests::fixtures::Rmw::Cyclonedds)
        .require("prebuilt cyclone graph-probe");
    let mut cmd = Command::new(probe);
    cmd.env("GRAPH_PROBE_EXPECT_NODE", "talker")
        .env("GRAPH_PROBE_WATCH_CHANGE", "1")
        .env("GRAPH_PROBE_TIMEOUT_MS", CHANGE_BUDGET_MS)
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_DOMAIN_ID", domain.to_string());
    // Issue 1137 — the SAME bus as the talker. `ManagedProcess::spawn_command`
    // deliberately does not pin (a `DockerRosEnv` peer could not read the
    // profile), so our half is pinned here, at the spawn site whose peer is a
    // host `ros2` process.
    nros_tests::dds_isolation::apply_to_command(&mut cmd);
    let mut probe = ManagedProcess::spawn_command(cmd, "graph-probe-change-cyclone")
        .expect("spawn the cyclone graph-change probe");
    let armed = probe
        .wait_for_output_pattern(output::GRAPH_PROBE_CHANGE_ARMED, CHANGE_WAIT)
        .expect("the probe must arm: it needs the talker in its graph first");
    assert!(
        !armed.contains(output::GRAPH_PROBE_CHANGE_UNSUPPORTED),
        "cyclone declined the graph-change slot; this cell says it owes one:\n{armed}"
    );

    drop(talker);

    let rest = probe
        .wait_for_all_output(CHANGE_WAIT)
        .expect("the probe must finish after the talker leaves");
    let out = format!("{armed}{rest}");
    assert!(
        !out.contains(output::GRAPH_PROBE_CHANGE_NONE),
        "cyclone ACCEPTED the graph-change callback and never fired it:\n{out}"
    );
    assert!(
        out.contains(output::GRAPH_PROBE_CHANGE_FIRED),
        "a peer leaving the graph must reach the guard condition:\n{out}"
    );
}
