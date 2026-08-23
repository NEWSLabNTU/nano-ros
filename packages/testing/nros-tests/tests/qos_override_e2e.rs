//! Issue #52 / 0303 / 0306 — a plan-level QoS override reaches a running
//! entity's ADVERTISED profile.
//!
//! ## Why this test exists
//!
//! Every layer of the override path had unit coverage and none of it was joined
//! up, which is how two bugs shipped in one week: the C and Rust emitters never
//! applied overrides at all (only C++ did), and the component runtime discarded
//! a node's own declared QoS (issue 0306) — so the plan could set QoS the code
//! could not. `ws-qos-rust` had demonstrated "the visible behaviour" of a QoS
//! profile for three phases while its e2e asserted only a message COUNT, which
//! default-to-default delivery satisfies just as well.
//!
//! ## What is observable, and what is not
//!
//! The zenoh backend does not implement QoS SEMANTICS: `to_qos_string`
//! (`nros-rmw-zenoh/src/keyexpr.rs`) encodes the profile into the liveliness
//! token and nothing else — no history cache, no depth-driven drop. So there is
//! no delivery difference to observe: transient_local does not replay to a late
//! joiner here, and a depth override does not change what arrives. A test that
//! claimed otherwise would be asserting a behaviour the stack does not have.
//!
//! What IS on the wire is the ADVERTISED profile, and a stock `rmw_zenoh_cpp`
//! peer reads it: `ros2 topic info --verbose` reports the discovered
//! publisher's QoS. That is the honest end of this chain, so that is what the
//! runtime half asserts.
//!
//! ## The oracle
//!
//! `reliable_talker_pkg` declares `reliable + transient_local + depth(10)` in
//! CODE. The committed model overrides RELIABILITY to `best_effort` — a value
//! the code never asks for. So on the wire:
//!
//! | reported reliability | meaning |
//! |---|---|
//! | `BEST_EFFORT` | the override reached the live entity — correct |
//! | `RELIABLE` | the override was dropped; the code's profile stands |
//!
//! and the transient_local half of the same report doubles as an 0306 guard: it
//! can only be there if the node's OWN declared QoS survived too.
//!
//! Run with: `cargo nextest run -p nros-tests --test qos_override_e2e`

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, build_native_workspace_rust_qos_entry, require_zenohd,
        zenohd_unique,
    },
    ros2::{DEFAULT_ROS_DISTRO, require_ros2},
    skip,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// The topic the talker publishes on.
const TOPIC: &str = "/qos_chatter";

/// ---------------------------------------------------------------------------
/// Half 1 — deterministic, no ROS 2: the DECLARATION still exists in the
/// bringup's inputs, and it lowers to the codes the bake will emit.
/// ---------------------------------------------------------------------------
///
/// This guards the fixture itself. The runtime half below skips without ROS 2,
/// so without this a silent edit to the bringup (or a regression in the
/// lowering) would leave the whole file green-by-skipping.
///
/// Reads `system.toml`, NOT a resolved model. phase-330 W4 made the SystemModel
/// a pure build artifact — regenerated into the active build's output dir and
/// no longer committed — so a test that opened
/// `config/rust_qos_model.yaml` failed on `os error 2` and said nothing about
/// the override (issue 0414). The declaration is an INPUT and always was; the
/// model only ever echoed it, and asserting on the echo tied this test to where
/// the build happened to leave a file.
#[test]
fn the_bringup_declares_a_reliability_override_that_lowers() {
    use nros_orchestration_ir::qos_override::{lower, policy, role};

    let system_toml = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../examples/workspaces/features/src/demo_bringup/system.toml"
    );
    let raw = std::fs::read_to_string(system_toml)
        .unwrap_or_else(|e| panic!("bringup input missing at {system_toml}: {e}"));
    let doc: toml::Value = toml::from_str(&raw).expect("system.toml parses");

    let key = "qos_overrides./qos_chatter.publisher.reliability";

    // The component that carries the override. Found by the KEY rather than by
    // component name: phase-331 W2b renamed these (issue 0398), and a test that
    // pins the name fails for a reason that has nothing to do with QoS.
    let value = doc
        .get("component")
        .and_then(|c| c.as_array())
        .expect("system.toml declares [[component]]")
        .iter()
        .find_map(|c| c.get("params")?.get(key)?.as_str())
        .unwrap_or_else(|| {
            panic!(
                "the fixture's whole point is this override — no [[component]] in \
                 {system_toml} declares params[\"{key}\"]"
            )
        });

    let lowered = lower(key, value)
        .expect("the override lowers")
        .expect("the key is an override");
    assert_eq!(lowered.topic, TOPIC);
    assert_eq!(lowered.role, role::PUBLISHER);
    assert_eq!(lowered.policy, policy::RELIABILITY);
    assert_eq!(lowered.value, 0, "best_effort is code 0");
}

/// ---------------------------------------------------------------------------
/// Half 2 — runtime: a stock ROS 2 peer reports the OVERRIDDEN profile.
/// ---------------------------------------------------------------------------
#[rstest]
fn a_ros2_peer_sees_the_overridden_publisher_profile(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        skip!("zenohd not found");
    }
    if !require_ros2() {
        skip!(
            "ROS 2 / rmw_zenoh_cpp not available — install it from apt \
             (`ros-$ROS_DISTRO-rmw-zenoh-cpp`, declared in nros-sdk-index.toml)."
        );
    }
    let locator = zenohd_unique.locator();

    let entry = build_native_workspace_rust_qos_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| skip!("qos workspace entry fixture not built: {e}"));
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        // Must outlast DISCOVERY_BUDGET below, or the wait races the talker's
        // own exit: the entry would go away mid-poll and the timeout would
        // report "never discovered" for a publisher that had simply stopped.
        .env("NROS_ENTRY_SPIN_MS", "45000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut talker = ManagedProcess::spawn_command(cmd, "qos_talker").expect("spawn qos entry");

    // Issue 0761 — poll, do not sleep-then-ask-once.
    //
    // This slept a fixed 3 s and queried once. In a 1658-test sweep the ros2
    // daemon had not finished discovering `/qos_chatter` within that window and
    // the test failed with `Unknown topic`, which reads like a discovery
    // REGRESSION; an immediate solo rerun on the same checkout and fixtures
    // passed in 5.08 s. Issue 0705 had already replaced this exact shape in
    // `workspace_features_e2e`; this is the site that sweep missed.
    //
    // The wait is bounded and the assertions below are untouched, so a real
    // regression still fails — just after 20 s instead of 3.
    const DISCOVERY_BUDGET: Duration = Duration::from_secs(20);
    let (report, found) = nros_tests::ros2::await_topic_endpoints(
        &locator,
        DEFAULT_ROS_DISTRO,
        TOPIC,
        // Issue 0690 — select by NODE, not by "first block of this kind". The
        // report is a flat list and 24 launch files publish on a `talker`, so
        // first-of-kind can read a SIBLING CELL's endpoint and assert against
        // its profile. The names come from `rust_qos.launch.xml`.
        &[
            ("PUBLISHER", "reliable_talker"),
            ("SUBSCRIPTION", "qos_listener"),
        ],
        DISCOVERY_BUDGET,
    )
    .unwrap_or_else(|e| {
        talker.kill();
        panic!("ros2 topic info failed: {e}")
    });
    talker.kill();

    let waited = DISCOVERY_BUDGET.as_secs();
    // Per-ENDPOINT assertions. A whole-report `contains` is useless here: the
    // report carries the publisher AND the subscription, so a substring match
    // passes on the wrong endpoint's profile. (Verified the hard way — an
    // earlier draft of this test passed with the issue-0306 fix reverted,
    // because the subscription's TRANSIENT_LOCAL satisfied the assertion while
    // the publisher's had been dropped to VOLATILE.)
    let one = |eps: &[nros_tests::ros2::TopicEndpoint], kind: &str, node: &str| -> String {
        match eps {
            [] => panic!(
                "ros2 did not discover the nros {kind} `{node}` on {TOPIC} within {waited}s.\n\
                 This is a DEADLINE, not a single shot (issue 0761), so the graph really did \
                 not carry it — check the entry started and the router locator matches, not \
                 discovery timing.\n{report}"
            ),
            [e] => e.block.clone(),
            many => panic!(
                "{} endpoints named `{node}` of kind {kind} on {TOPIC} — a sibling cell is \
                 publishing into this graph (issue 0690), so asserting a profile here would \
                 read someone else's.\n{report}",
                many.len()
            ),
        }
    };
    let publisher = one(&found[0], "PUBLISHER", "reliable_talker");
    let subscription = one(&found[1], "SUBSCRIPTION", "qos_listener");

    // The override is the ONLY possible source of BEST_EFFORT — the node's code
    // declares `reliable`.
    assert!(
        publisher.contains("Reliability: BEST_EFFORT"),
        "the plan's `qos_overrides.{TOPIC}.publisher.reliability = best_effort` did not reach the \
         live entity; the publisher advertises the code's own profile:\n{publisher}"
    );
    // Issue 0306 — the node's OWN declared QoS must survive alongside the
    // override: `transient_local` and depth 10 come from the code, not the plan.
    assert!(
        publisher.contains("Durability: TRANSIENT_LOCAL"),
        "the node's code-declared durability was dropped (issue 0306 regression): the plan \
         override applied but the declared profile did not:\n{publisher}"
    );
    assert!(
        publisher.contains("KEEP_LAST (10)"),
        "the node's code-declared depth was dropped (issue 0306 regression):\n{publisher}"
    );

    // Role targeting: the override names `publisher`, so the SUBSCRIPTION must
    // keep the code's RELIABLE. If overrides leaked across roles this is where
    // it shows.
    assert!(
        subscription.contains("Reliability: RELIABLE"),
        "a publisher-scoped override leaked onto the subscription:\n{subscription}"
    );
    assert!(
        subscription.contains("Durability: TRANSIENT_LOCAL"),
        "the subscription's code-declared durability was dropped (issue 0306 \
         regression):\n{subscription}"
    );
}

// phase-329 W3 — bind this test to `interop::CELLS` (the pattern from
// xrce_ros2_interop). The coordinate below must equal what the list declares
// for `qos_override_e2e`; drift turns this RED. Needs no fixtures — runs in tier 1.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound("qos_override_e2e", &[(Linux, Rust, Zenoh, Qos)]);
}
