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
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
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
        skip!("ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup");
    }
    let locator = zenohd_unique.locator();

    let entry = build_native_workspace_rust_qos_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| skip!("qos workspace entry fixture not built: {e}"));
    let mut cmd = Command::new(entry);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", &locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "20000")
        .env("NROS_ENTRY_SPIN_STEP_MS", "10");
    let mut talker = ManagedProcess::spawn_command(cmd, "qos_talker").expect("spawn qos entry");

    // The publisher must exist and its liveliness token must have propagated
    // before `ros2 topic info` can report anything.
    std::thread::sleep(Duration::from_secs(3));

    // The TempDir holds the rmw_zenoh session config alive for the child.
    let (setup, _cfg) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, &locator);
    let out = Command::new("bash")
        .arg("-lc")
        .arg(format!("{setup} && ros2 topic info --verbose {TOPIC}"))
        .output()
        .expect("run ros2 topic info");
    talker.kill();

    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Per-ENDPOINT assertions. A whole-report `contains` is useless here: the
    // report carries the publisher AND the subscription, so a substring match
    // passes on the wrong endpoint's profile. (Verified the hard way — an
    // earlier draft of this test passed with the issue-0306 fix reverted,
    // because the subscription's TRANSIENT_LOCAL satisfied the assertion while
    // the publisher's had been dropped to VOLATILE.)
    let publisher = endpoint_block(&report, "PUBLISHER").unwrap_or_else(|| {
        panic!("ros2 did not discover the nros publisher on {TOPIC}:\n{report}")
    });
    let subscription = endpoint_block(&report, "SUBSCRIPTION").unwrap_or_else(|| {
        panic!("ros2 did not discover the nros subscription on {TOPIC}:\n{report}")
    });

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

/// Slice one `Endpoint type: <kind>` section out of `ros2 topic info --verbose`
/// output, up to the next blank-line-separated endpoint.
fn endpoint_block(report: &str, kind: &str) -> Option<String> {
    let marker = format!("Endpoint type: {kind}");
    let start = report.find(&marker)?;
    let rest = &report[start..];
    // Each endpoint's QoS block ends at the next "Node name:" (the following
    // endpoint) or at the end of the report.
    let end = rest[marker.len()..]
        .find("Node name:")
        .map(|i| i + marker.len())
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
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
