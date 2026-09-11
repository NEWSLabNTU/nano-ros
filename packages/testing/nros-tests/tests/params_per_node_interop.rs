//! phase-426 W6 — the cell that would have caught three parameter stores.
//!
//! `tests/params.rs` already drives `ros2 param list/get/set` against a nano-ros
//! image, and it passed for the whole time the defect existed, because it drives
//! a ONE-node image. One node is the blind spot: the six REP-2002 parameter
//! services used to be registered under the EXECUTOR's fully-qualified name and
//! backed by one flat table, and an image with a single node cannot tell that
//! apart from a set per node over a table keyed by node. Everything phase-426
//! changed is invisible at N = 1.
//!
//! So this cell runs the same three verbs against a TWO-node image
//! (`param-two-node-talker`: `alpha` and `beta` on one executor) and asks the
//! questions that need two:
//!
//! * both node FQNs appear in `ros2 param list` — W3's acceptance, on the wire;
//! * a `get` on each returns THAT node's value where both declare the same
//!   parameter name, and a `set` on one does not move the other — W1;
//! * a `set` on a name the node never declared is REFUSED — issue 1151;
//! * a `set` off the declared `step` is REFUSED — issue 1150.
//!
//! The in-process half of the same claim is
//! `Executor::parameter_service_node_names` (W3). This file is the WIRE half:
//! the fixture prints that list at boot, and each test asserts the ROS graph
//! agrees with it. Both ends of one claim, so a divergence names its own side.
//!
//! Live peer: `ros2` from the host's ROS 2 install, over an ephemeral `zenohd`.
//! Runs from `just native test-ros2-params-per-node`; cell
//! `native-params-per-node-rust-zenoh` in `interop::CELLS`.

use nros_tests::{
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_native_param_two_node_talker,
        build_native_param_two_node_talker_cyclonedds, require_ros2, require_zenohd,
        zenohd_unique,
    },
    output::PARAM_SERVICE_NODE_PREFIX,
    ros2::DEFAULT_ROS_DISTRO,
};
use rstest::rstest;
use std::{process::Command, time::Duration};

/// What `ros2 param set` prints on success. Anything else — a refusal, a
/// timeout, a traceback — is not this string.
const SET_OK: &str = "Set parameter successful";

/// A running two-node image, plus the FQNs it says it registered services under.
struct TwoNodeImage {
    proc: ManagedProcess,
    /// In registration order, from the fixture's own
    /// `parameter_service_node_names` — `alpha` first, then `beta`.
    fqns: Vec<String>,
}

impl TwoNodeImage {
    fn alpha(&self) -> &str {
        &self.fqns[0]
    }

    fn beta(&self) -> &str {
        &self.fqns[1]
    }
}

impl Drop for TwoNodeImage {
    fn drop(&mut self) {
        self.proc.kill();
    }
}

/// Boot the fixture and read back the node FQNs it registered parameter
/// services under.
///
/// The wait is on the COUNT of marker lines, not on one line: a tree where the
/// per-node registration is gone still prints a first marker, promptly, and a
/// wait for "a marker" would be satisfied by it and then fail three assertions
/// later with the fixture already several seconds into its spin. Waiting for two
/// puts the failure where the cause is, and the diagnostic carries what the
/// process actually printed.
fn start_two_node_image(locator: &str) -> TwoNodeImage {
    let binary = build_native_param_two_node_talker().require("param-two-node-talker");

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client");

    let mut proc =
        ManagedProcess::spawn_command(cmd, "param-two-node").expect("failed to start fixture");

    let (output, diag) =
        proc.collect_until_count(PARAM_SERVICE_NODE_PREFIX, 2, Duration::from_secs(20));
    let fqns: Vec<String> = output
        .lines()
        .filter_map(|l| l.split_once(PARAM_SERVICE_NODE_PREFIX))
        .map(|(_, fqn)| fqn.trim().to_string())
        .collect();

    // A timed-out wait returns NO output (issue 0670 keeps the diagnostic and
    // the asserted string apart on purpose), so this branch cannot report a
    // count — the diagnostic is where the evidence is.
    if let Some(diag) = diag {
        panic!(
            "the image composes two nodes, so it must register two sets of \
             parameter services and print two `{PARAM_SERVICE_NODE_PREFIX}` \
             lines, and the wait for the second timed out. Seeing exactly ONE \
             means the six services are pinned to a single node key again \
             (phase-426 W3) — which is the state where `ros2 param list` \
             enumerates the executor and never the image's nodes.\n{diag}"
        );
    }
    assert_eq!(
        fqns.len(),
        2,
        "the image composes two nodes, so it must register two sets of parameter \
         services; parsed {fqns:?}.\nOutput:\n{output}"
    );
    assert!(
        fqns[0].ends_with("/alpha") && fqns[1].ends_with("/beta"),
        "the fixture's nodes are `alpha` and `beta`, in that order; got {fqns:?}"
    );
    assert_ne!(
        fqns[0], fqns[1],
        "two nodes sharing one FQN is the pre-W3 keying wearing a plural"
    );

    // Discovery through the router takes a moment after the services attach.
    std::thread::sleep(Duration::from_secs(1));

    TwoNodeImage { proc, fqns }
}

/// Require both FQNs in `ros2 node list`; skip loudly if the graph never shows
/// them.
///
/// Returns normally ONLY on success (issue 1135 — a `require_*` that hands back
/// a `bool` hands back the verdict, and a bare `return` from a `#[test]` is a
/// PASS). Everything cheap has already passed by the time this runs, so a
/// negative here is "zenohd is up, ROS 2 is up, our image is up, and it is not
/// in the graph": a delivery failure, classed `resource`.
fn require_both_nodes_discoverable(image: &TwoNodeImage, locator: &str) {
    let mut last = String::new();
    for attempt in 1..=5 {
        if let Ok(list) = nros_tests::ros2::ros2_node_list(locator, DEFAULT_ROS_DISTRO) {
            let seen: Vec<&str> = list.lines().map(str::trim).collect();
            if seen.contains(&image.alpha()) && seen.contains(&image.beta()) {
                return;
            }
            last = list;
        }
        if attempt < 5 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    nros_tests::skip_class!(
        resource,
        "the image's nodes ({}, {}) are not both in `ros2 node list` after 5 attempts \
         (locator {locator}). zenohd, ROS 2 and the fixture were all present, so this is \
         our image failing to reach the ROS graph, not a missing prerequisite. \
         Last listing:\n{last}",
        image.alpha(),
        image.beta()
    );
}

/// `ros2 param list <node>`, retried while discovery settles.
///
/// The retry is on the CONTENT, not on the call: `ros2 param list` against a
/// node whose services have not been discovered yet exits 0 with a traceback on
/// stdout, so "the command ran" says nothing.
fn param_list_until(node: &str, expect: &str, locator: &str) -> String {
    let mut out = String::new();
    for attempt in 1..=4 {
        out = nros_tests::ros2::ros2_param_list(node, locator, DEFAULT_ROS_DISTRO)
            .expect("failed to run ros2 param list");
        if out.contains(expect) {
            break;
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    out
}

fn param_get(node: &str, name: &str, locator: &str) -> String {
    nros_tests::ros2::ros2_param_get(node, name, locator, DEFAULT_ROS_DISTRO)
        .expect("failed to run ros2 param get")
}

fn param_set(node: &str, name: &str, value: &str, locator: &str) -> String {
    nros_tests::ros2::ros2_param_set(node, name, value, locator, DEFAULT_ROS_DISTRO)
        .expect("failed to run ros2 param set")
}

/// W3 + W1 on the wire: each node is addressable by its OWN fully-qualified
/// name, and answers with its OWN value for a parameter name they share.
#[rstest]
fn ros2_param_cli_addresses_each_node_by_its_own_fqn(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }
    let locator = zenohd_unique.locator();
    let image = start_two_node_image(&locator);
    require_both_nodes_discoverable(&image, &locator);

    // --- W3's acceptance, on the wire: the enumerating form sees BOTH nodes.
    let mut all = String::new();
    for attempt in 1..=4 {
        all = nros_tests::ros2::ros2_param_list_all(&locator, DEFAULT_ROS_DISTRO)
            .expect("failed to run ros2 param list");
        if all.contains(&format!("{}:", image.alpha()))
            && all.contains(&format!("{}:", image.beta()))
        {
            break;
        }
        if attempt < 4 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    println!("=== ros2 param list (whole graph) ===\n{all}");
    assert!(
        all.contains(&format!("{}:", image.alpha())),
        "`ros2 param list` should enumerate {}. Output:\n{all}",
        image.alpha()
    );
    assert!(
        all.contains(&format!("{}:", image.beta())),
        "`ros2 param list` should enumerate {} as well — one set of six services \
         PER NODE is what phase-426 W3 registers, and an image whose second node \
         is missing here is serving the executor's identity, not its nodes. \
         Output:\n{all}",
        image.beta()
    );

    // --- Each node lists its own parameters, and only its own.
    let alpha_list = param_list_until(image.alpha(), "stepped", &locator);
    println!("=== ros2 param list {} ===\n{alpha_list}", image.alpha());
    assert!(
        alpha_list.contains("rate") && alpha_list.contains("stepped"),
        "{} declares both `rate` and `stepped`. Output:\n{alpha_list}",
        image.alpha()
    );

    let beta_list = param_list_until(image.beta(), "rate", &locator);
    println!("=== ros2 param list {} ===\n{beta_list}", image.beta());
    assert!(
        beta_list.contains("rate"),
        "{} declares `rate`. Output:\n{beta_list}",
        image.beta()
    );
    assert!(
        !beta_list.contains("stepped"),
        "`stepped` is {}'s alone; {} showing it means the two nodes share one \
         flat table (the pre-W1 store) and the FQNs are decoration. Output:\n{beta_list}",
        image.alpha(),
        image.beta()
    );

    // --- The same NAME on both nodes returns each node's own value.
    let alpha_rate = param_get(image.alpha(), "rate", &locator);
    let beta_rate = param_get(image.beta(), "rate", &locator);
    println!("=== ros2 param get rate ===\n{alpha_rate}\n{beta_rate}");
    assert!(
        alpha_rate.contains("Integer value is: 10"),
        "{} declared rate=10. Output:\n{alpha_rate}",
        image.alpha()
    );
    assert!(
        beta_rate.contains("Integer value is: 20"),
        "{} declared rate=20; a table keyed by NAME alone would answer 10 here \
         (or whichever declaration landed last). Output:\n{beta_rate}",
        image.beta()
    );

    // --- A write to one node leaves its sibling alone (W2 through W1's keying).
    let set_out = param_set(image.beta(), "rate", "21", &locator);
    println!("=== ros2 param set {} rate 21 ===\n{set_out}", image.beta());
    assert!(
        set_out.contains(SET_OK),
        "setting a declared, in-range parameter should succeed. Output:\n{set_out}"
    );
    let beta_after = param_get(image.beta(), "rate", &locator);
    assert!(
        beta_after.contains("Integer value is: 21"),
        "the set should be readable back on {}. Output:\n{beta_after}",
        image.beta()
    );
    let alpha_after = param_get(image.alpha(), "rate", &locator);
    assert!(
        alpha_after.contains("Integer value is: 10"),
        "writing {}'s `rate` must not move {}'s — one flat table is exactly the \
         collision phase-426 W1 fixed. Output:\n{alpha_after}",
        image.beta(),
        image.alpha()
    );
}

/// Issues 1151 and 1150 on the wire: a `set` the store must refuse, and a
/// control showing the refusals are a constraint rather than a broken writer.
#[rstest]
fn ros2_param_set_refuses_undeclared_and_off_step(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !require_ros2() {
        nros_tests::skip!("ROS 2 not found");
    }
    let locator = zenohd_unique.locator();
    let image = start_two_node_image(&locator);
    require_both_nodes_discoverable(&image, &locator);

    // Settle discovery on the node under test before asserting a REFUSAL: an
    // undiscovered node refuses everything, which would pass this test for the
    // wrong reason.
    let listed = param_list_until(image.alpha(), "stepped", &locator);
    assert!(
        listed.contains("stepped"),
        "{}'s parameter services must be reachable before a refusal means \
         anything. Output:\n{listed}",
        image.alpha()
    );

    // --- Issue 1151: a name the node never declared.
    let undeclared = param_set(image.alpha(), "never_declared", "7", &locator);
    println!("=== ros2 param set never_declared 7 ===\n{undeclared}");
    assert!(
        !undeclared.contains(SET_OK),
        "`ros2 param set` on an undeclared name must be REFUSED (issue 1151): a \
         typo reported success and created a second, invisible parameter while \
         the real one kept its value. Output:\n{undeclared}"
    );
    let after_undeclared =
        nros_tests::ros2::ros2_param_list(image.alpha(), &locator, DEFAULT_ROS_DISTRO)
            .expect("failed to run ros2 param list");
    assert!(
        !after_undeclared.contains("never_declared"),
        "the refused name must not appear in the node's parameters. Output:\n{after_undeclared}"
    );

    // --- Issue 1150: `stepped` is 0..=100 on a step-5 lattice; 12 is off it.
    let off_step = param_set(image.alpha(), "stepped", "12", &locator);
    println!("=== ros2 param set stepped 12 ===\n{off_step}");
    assert!(
        !off_step.contains(SET_OK),
        "12 is off the declared step-5 lattice and must be REFUSED (issue 1150): \
         `step` was stored and published to `ros2 param describe` while nothing \
         enforced it. Output:\n{off_step}"
    );
    let still = param_get(image.alpha(), "stepped", &locator);
    assert!(
        still.contains("Integer value is: 10"),
        "a refused set must leave the value alone. Output:\n{still}"
    );

    // --- The control. Without it, a writer that refuses EVERYTHING passes the
    //     two assertions above, and the cell would be measuring nothing.
    let on_step = param_set(image.alpha(), "stepped", "15", &locator);
    println!("=== ros2 param set stepped 15 ===\n{on_step}");
    assert!(
        on_step.contains(SET_OK),
        "15 is on the lattice and in range, so it must be ACCEPTED — the two \
         refusals above only mean something if the writer works. Output:\n{on_step}"
    );
    let moved = param_get(image.alpha(), "stepped", &locator);
    assert!(
        moved.contains("Integer value is: 15"),
        "the accepted set should be readable back. Output:\n{moved}"
    );
}

// phase-329 W3 — bind this test to `interop::CELLS`. The coordinate below must
// equal what the list declares for `params_per_node_interop`; drift turns this
// RED. Needs no fixtures, so it runs in tier 1.
/// issue 1268 / phase-444 W6 — the SAME claim on CYCLONE.
///
/// THE DEFECT. Cyclone creates a service only if its request and reply types
/// have a registered descriptor. Nothing registered `rcl_interfaces`, so all six
/// parameter services failed to create with UNSUPPORTED, the executor retried
/// that permanent failure on every spin, and `ros2 param` found nothing on an
/// image that had declared every one of its parameters — while the zenoh cases
/// above stayed green throughout. That is why this is a SECOND test rather than
/// a wider one: a green zenoh case says nothing about Cyclone (issue 1269 gave
/// the same reason for `native-multinode-rust-cyclone`).
///
/// Addressed by DOMAIN, not a locator: Cyclone discovers by SPDP. Its own domain,
/// because a shared one would let another test's nodes into the listing; and
/// pinned to loopback on BOTH sides — `dds_isolation::apply_to_command` for ours,
/// the env string for the peer's — since half a pin is no discovery and reads as
/// an empty graph rather than as a failure (issues 1009 / 1137).
#[test]
fn ros2_param_cli_addresses_each_node_on_cyclonedds() -> nros_tests::TestResult<()> {
    if !nros_tests::ros2::require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    let binary = build_native_param_two_node_talker_cyclonedds()
        .unwrap_or_else(|e| panic!("param-two-node-talker-cyclone fixture not built: {e}"));

    let domain = nros_tests::unique_ros_domain_id();
    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("NROS_DOMAIN_ID", domain.to_string())
        // The registered backend NAME, never an ambient lane token (AGENTS.md
        // "`NROS_RMW` footgun").
        .env("NROS_RMW", "cyclonedds");
    nros_tests::dds_isolation::apply_to_command(&mut cmd);

    let mut proc = ManagedProcess::spawn_command(cmd, "param-two-node-cyclone")
        .expect("failed to start fixture");
    let (output, diag) =
        proc.collect_until_count(PARAM_SERVICE_NODE_PREFIX, 2, Duration::from_secs(20));
    let fqns: Vec<String> = output
        .lines()
        .filter_map(|l| l.split_once(PARAM_SERVICE_NODE_PREFIX))
        .map(|(_, fqn)| fqn.trim().to_string())
        .collect();
    if let Some(diag) = diag {
        proc.kill();
        panic!(
            "the image composes two nodes, so it must register two sets of \
             parameter services and print two `{PARAM_SERVICE_NODE_PREFIX}` lines. \
             On Cyclone this is ALSO where issue 1268 shows: a registration that \
             fails with UNSUPPORTED prints no marker at all.\n{diag}"
        );
    }
    assert_eq!(
        fqns.len(),
        2,
        "expected two registered node FQNs, got {fqns:?}"
    );
    let (alpha, beta) = (fqns[0].clone(), fqns[1].clone());

    // THE ASSERTION 1268 IS ABOUT: the six services exist on the wire. Before the
    // descriptors were baked this listing carried the image's own services and
    // none of the `rcl_interfaces` ones.
    let mut services = String::new();
    for attempt in 1..=6 {
        services = nros_tests::ros2::ros2_service_list_rmw_with_domain(
            DEFAULT_ROS_DISTRO,
            "rmw_cyclonedds_cpp",
            domain,
        )
        .unwrap_or_default();
        if services.contains(&format!("{alpha}/list_parameters")) {
            break;
        }
        if attempt < 6 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    println!("=== ros2 service list (cyclonedds) ===\n{services}");
    for node in [&alpha, &beta] {
        for svc in [
            "list_parameters",
            "get_parameters",
            "set_parameters",
            "set_parameters_atomically",
            "describe_parameters",
            "get_parameter_types",
        ] {
            assert!(
                services.contains(&format!("{node}/{svc}")),
                "cyclonedds: `{node}/{svc}` is missing from `ros2 service list`. \
                 That is issue 1268: the service create fails UNSUPPORTED when the \
                 `rcl_interfaces` type has no registered descriptor.\n{services}"
            );
        }
    }

    // And the round trip the issue's Acceptance names.
    let got = nros_tests::ros2::ros2_param_get_rmw_with_domain(
        &alpha,
        "rate",
        DEFAULT_ROS_DISTRO,
        "rmw_cyclonedds_cpp",
        domain,
    )
    .expect("failed to run ros2 param get");
    proc.kill();
    println!("=== ros2 param get {alpha} rate (cyclonedds) ===\n{got}");
    assert!(
        got.contains("Double value is") || got.contains("Integer value is"),
        "cyclonedds: `ros2 param get {alpha} rate` returned no value:\n{got}"
    );
    Ok(())
}

#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "params_per_node_interop",
        &[(Linux, Rust, Zenoh, Params)],
    );
}
