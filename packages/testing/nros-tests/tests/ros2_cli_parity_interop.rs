//! The same `ros2` CLI question, asked of a nano-ros node and of a stock ROS 2
//! node that declare the same interface, and the two answers compared
//! (phase-556, the ROS 2 interface compatibility audit).
//!
//! Each side runs alone on its own `rmw_zenohd`, so a graph-wide query answers
//! about one implementation (`nros_tests::ros2_compat`). Answers are compared
//! after each side's own node name is replaced by `<node>`; nothing else is
//! normalised unless a call site says so.
//!
//! Two kinds of test live here:
//!
//! * **Plain** — the axis matches stock, and the test asserts the two
//!   answers are equal.
//! * **Expected-failure** — a measured gap, tracked by an issue named in the
//!   test's source comment. The test asserts the recorded difference is still
//!   there, so it stays green while the gap is real, and it FAILS the day the
//!   answers agree, telling whoever fixed it to turn it into the plain check
//!   ([`hold_gap`]). It is never a skip.
//!
//! The pairs (nano-ros fixture : stock peer):
//!
//! | cell | nano-ros | stock |
//! |---|---|---|
//! | params | `param-two-node-talker` (`/alpha`, `/beta`) | `fixtures/ros2-param-peer/param_peer.py` (rclpy, the same parameters as `/alpha`) |
//! | service | rust workspace `linux_service_server` (`/add_server`) | `demo_nodes_cpp add_two_ints_server` |
//! | action | rust workspace `linux_action_server` (`/fibonacci_server`) | `examples_rclcpp_minimal_action_server` |
//! | pubsub | `examples/linux/rust/talker` (`/talker`) | `demo_nodes_cpp talker` |
//!
//! Live peer: the host's ROS 2 (Humble) with `rmw_zenoh_cpp`. Run with
//! `just linux test-ros2-cli-parity`; cells `native-ros2-cli-parity-*` in
//! `interop::CELLS`. Nothing compiles at run time: build the four fixture rows
//! first (`just linux build-fixtures`).

use nros_tests::{
    fixtures::{
        RequireFixture, build_native_action_server, build_native_param_two_node_talker,
        build_native_service_server, build_native_talker, require_ros2, require_zenohd,
    },
    ros2::{DEFAULT_ROS_DISTRO, is_ros2_package_available},
    ros2_compat::{
        CliAnswer, Compared, GraphSide, Implementation, NODE_PLACEHOLDER, Pair, changed_lines,
        is_parameter_service_line, only_lines_containing, parameter_service_lines,
        without_lines_containing,
    },
};
use std::time::Duration;

const SETTLE: Duration = Duration::from_secs(20);

/// A stock package the peer needs. Missing is an unmet precondition of the
/// host, named with its apt package.
fn require_stock_package(pkg: &str) {
    if !is_ros2_package_available(DEFAULT_ROS_DISTRO, pkg) {
        nros_tests::unmet!(
            "the stock ROS 2 package `{pkg}` is not installed (apt: \
             `ros-{DEFAULT_ROS_DISTRO}-{}`); the compatibility tests compare a nano-ros \
             node against it",
            pkg.replace('_', "-")
        );
    }
}

/// Holds a measured difference between nano-ros and stock in expected-failure
/// form. `gap_present` recognises the recorded difference.
///
/// * the answers agree: the gap is fixed, and the test must be flipped to the
///   plain check, so this FAILS saying so;
/// * the recorded difference is there: passes;
/// * they differ some other way: FAILS, because the test no longer measures
///   what it records.
fn hold_gap(c: &Compared, what: &str, gap_present: impl Fn(&Compared) -> bool) {
    let Some(diff) = c.diff() else {
        panic!(
            "{what}: nano-ros now answers `ros2 {}` exactly as stock ROS 2 does. This test \
             held that difference as a known gap so that a fix would be noticed. Change it to \
             the plain compatibility check, asserting the two answers are equal \
             (`Compared::assert_same`), and close the gap it tracked.\n\n{}",
            c.query,
            c.report()
        );
    };
    if !gap_present(c) {
        panic!(
            "{what}: nano-ros and stock ROS 2 still answer `ros2 {}` differently, but not in \
             the way this test records (- nano-ros only, + stock only):\n{diff}\n\nIf nano-ros \
             changed this answer on purpose to match stock, change this test to the plain \
             compatibility check (`Compared::assert_same`). Otherwise the answer moved and \
             the recorded difference needs measuring again.\n\n{}",
            c.query,
            c.report()
        );
    }
}

/// The plain-form counterpart of [`hold_gap`] for a predicate that is not an
/// equality: fails with `what` when `still_differs` is false.
fn hold_gap_by(still_differs: bool, what: &str, query: &str, evidence: &str) {
    if !still_differs {
        panic!(
            "{what}: nano-ros now answers `ros2 {query}` as stock ROS 2 does. This test held \
             that difference as a known gap so that a fix would be noticed. Change it to the \
             plain compatibility check and close the gap it tracked.\n\n{evidence}"
        );
    }
}

fn has_line(a: &CliAnswer, needle: &str) -> bool {
    a.output.lines().any(|l| l.contains(needle))
}

// ── the params pair ─────────────────────────────────────────────────────────

/// nano-ros: the two-node parameter image, asked about `/alpha`. Stock: the
/// rclpy node declaring `/alpha`'s parameters.
fn param_pair() -> Pair {
    let nano_bin = build_native_param_two_node_talker().require("param-two-node-talker");
    require_zenohd();
    require_ros2();
    let peer = nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/ros2-param-peer/param_peer.py");

    let mut nano = GraphSide::new(Implementation::NanoRos, "/alpha");
    nano.spawn_nano_image(nano_bin, "param-two-node-talker");
    nano.await_node();

    let mut stock = GraphSide::new(Implementation::Stock, "/param_peer");
    stock.spawn_stock(
        &format!("python3 -u {} param_peer", peer.display()),
        "stock rclpy parameter node",
    );
    stock.await_node();
    Pair::new(nano, stock)
}

/// Ready once the side's `param list` shows `stepped`, i.e. its parameter
/// services are discovered.
fn params_discovered(pair: &Pair) {
    let c = pair.ask_until("param list {node}", SETTLE, |a| has_line(a, "stepped"));
    for (side, a) in [("nano-ros", &c.nano), ("stock", &c.stock)] {
        assert!(
            has_line(a, "stepped"),
            "the {side} node's parameter services were never discovered:\n{a}"
        );
    }
}

/// Matches stock: `ros2 param list`, `list --param-type`, `get`, `describe`,
/// `dump` and an accepted `set`, on declared parameters.
///
/// `use_sim_time` is set aside here: stock declares it and nano-ros does not,
/// which `use_sim_time_compared_with_stock` holds (issue 2483).
#[test]
fn declared_parameters_answer_ros2_param_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);
    let no_sim_time = || without_lines_containing(&["use_sim_time"]);

    pair.ask("param list {node}")
        .filtered(no_sim_time())
        .assert_same("the declared parameter names");
    pair.ask("param list --param-type {node}")
        .filtered(no_sim_time())
        .assert_same("the declared parameter names and types");
    pair.ask("param get {node} rate")
        .assert_same("the value of an integer parameter");
    pair.ask("param get {node} stepped")
        .assert_same("the value of a range-constrained parameter");
    pair.ask("param describe {node} rate stepped")
        .assert_same("the descriptors of two declared parameters");
    pair.ask("param dump {node}")
        .filtered(no_sim_time())
        .assert_same("the dump of the declared parameters");
    pair.ask("param set {node} rate 11")
        .assert_same("an accepted set of a declared parameter");
    pair.ask_until("param get {node} rate", SETTLE, |a| has_line(a, "11"))
        .assert_same("the value read back after the set");
}

/// Matches stock: the six parameter services of a node, names and types, as
/// `ros2 service list -t` lists them.
#[test]
fn first_node_serves_the_six_parameter_services_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);
    let own = |node: String| {
        move |a: &CliAnswer| has_line(a, &format!("{node}/set_parameters_atomically ["))
    };
    let c = pair
        .ask_each(
            "service list -t",
            SETTLE,
            own(pair.nano.node.clone()),
            own(pair.stock.node.clone()),
        )
        .filtered(|l| l.starts_with(&format!("{NODE_PLACEHOLDER}/")));
    assert_eq!(
        c.stock_text.lines().map(str::to_string).collect::<Vec<_>>(),
        parameter_service_lines(),
        "a stock node serves exactly the six parameter services.\n{}",
        c.report()
    );
    c.assert_same("the parameter services of a node");
}

/// Matches stock: each node of a two-node image serves the six parameter
/// services `ros2 node info` lists for a stock node.
///
/// Issue 2482 (resolved): the second node served 5 of the 6. The image's
/// liveliness pool was sized for one node's services, so `/beta`'s last
/// service had no graph token and `ros2 node info` omitted it.
#[test]
fn each_node_of_a_two_node_image_serves_the_parameter_services_as_stock_does() {
    let mut pair = param_pair();
    params_discovered(&pair);
    // Ready once the side lists its LAST-declared parameter service, so a
    // half-discovered node is not compared.
    let ready = |a: &CliAnswer| has_line(a, "/get_parameter_types: ");

    for node in ["/alpha", "/beta"] {
        pair.nano.node = node.to_string();
        pair.ask_until("node info {node}", SETTLE, ready)
            .filtered(is_parameter_service_line)
            .assert_same(&format!(
                "the parameter services `ros2 node info` lists for node {node} of a \
                 two-node image"
            ));
    }
}

/// Expected-failure: every stock node publishes its own `/parameter_events`;
/// in a two-node nano-ros image only the first node does.
// design gap: one /parameter_events writer per executor (issue 2023); separate issue pending
#[test]
fn second_node_as_parameter_events_publisher_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);

    // The query is graph-wide, so the publishers are read per side rather than
    // diffed: the nano-ros graph holds two nodes and the stock graph one.
    let publishers = |side: &GraphSide| {
        let a = side.query_until("topic info -v /parameter_events", SETTLE, |a| {
            has_line(a, "Endpoint type: PUBLISHER")
        });
        let names: Vec<String> = a
            .output
            .lines()
            .filter_map(|l| l.strip_prefix("Node name: "))
            .filter(|n| !n.starts_with("_ros2cli_"))
            .map(|n| format!("/{}", n.trim()))
            .collect();
        (names, a)
    };
    let (stock_pubs, stock_answer) = publishers(&pair.stock);
    assert!(
        stock_pubs.contains(&pair.stock.node),
        "a stock node publishes its own /parameter_events.\n{stock_answer}"
    );
    let (nano_pubs, nano_answer) = publishers(&pair.nano);
    assert!(
        nano_pubs.iter().any(|n| n == "/alpha"),
        "the first node of the image publishes /parameter_events.\n{nano_answer}"
    );
    hold_gap_by(
        !nano_pubs.iter().any(|n| n == "/beta"),
        "the second node of an image as a /parameter_events publisher",
        "topic info -v /parameter_events",
        &format!("nano-ros answered:\n{nano_answer}\n\nstock ROS 2 answered:\n{stock_answer}"),
    );
}

/// Expected-failure, issue 2483: every stock node declares `use_sim_time`
/// (bool, false); a nano-ros node with parameter services does not.
#[test]
fn use_sim_time_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);

    let c = pair.ask("param list {node}");
    hold_gap(&c, "`use_sim_time` in the parameter list", |c| {
        c.diff()
            .is_some_and(|d| changed_lines(&d) == vec!["+   use_sim_time".to_string()])
    });

    let c = pair.ask("param get {node} use_sim_time");
    hold_gap(&c, "the value of `use_sim_time`", |c| {
        c.stock_text.trim() == "Boolean value is: False" && !c.nano_text.contains("Boolean value")
    });
}

/// Expected-failure, issue 2484: `GetParameters` for an undeclared name. Stock
/// replies with no entry and `ros2 param get` exits 1; nano-ros replies with
/// a filler entry of type 0 and the CLI exits 0.
#[test]
fn get_parameters_reply_for_an_undeclared_name_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);

    let c = pair.ask_until(
        "service call {node}/get_parameters rcl_interfaces/srv/GetParameters '{names: [nope]}'",
        SETTLE,
        |a| has_line(a, "GetParameters_Response("),
    );
    hold_gap(&c, "the GetParameters reply for an undeclared name", |c| {
        c.stock_text.contains("GetParameters_Response(values=[])")
            && c.nano_text
                .contains("GetParameters_Response(values=[rcl_interfaces.msg.ParameterValue(type=0")
    });

    let c = pair.ask("param get {node} nope");
    hold_gap(
        &c,
        "the exit status of `ros2 param get` for an undeclared name",
        |c| c.stock.exit_code == Some(1) && c.nano.exit_code == Some(0),
    );
}

/// Expected-failure, issue 2484: `DescribeParameters` for an undeclared name.
/// Stock replies with no descriptor and `ros2 param describe` prints nothing;
/// nano-ros replies with a filler descriptor the CLI prints as `Type: not
/// set`.
#[test]
fn describe_parameters_reply_for_an_undeclared_name_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);

    let c = pair.ask_until(
        "service call {node}/describe_parameters rcl_interfaces/srv/DescribeParameters \
         '{names: [nope]}'",
        SETTLE,
        |a| has_line(a, "DescribeParameters_Response("),
    );
    hold_gap(
        &c,
        "the DescribeParameters reply for an undeclared name",
        |c| {
            c.stock_text
                .contains("DescribeParameters_Response(descriptors=[])")
                && c.nano_text.contains(
                    "DescribeParameters_Response(descriptors=[rcl_interfaces.msg.\
                 ParameterDescriptor(name='nope', type=0",
                )
        },
    );

    let c = pair.ask_until("param describe {node} nope", SETTLE, |_| true);
    hold_gap(&c, "`ros2 param describe` of an undeclared name", |c| {
        c.stock_text.is_empty() && c.nano_text.contains("Parameter name: nope")
    });
}

/// Expected-failure, issue 2484: deleting a statically typed parameter is
/// refused by both, with different reasons.
#[test]
fn delete_refusal_reason_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);
    let c = pair.ask("param delete {node} rate");
    hold_gap(&c, "the reason a parameter delete is refused", |c| {
        c.nano_text.trim() == "Deleting parameter failed: Type mismatch"
            && c.stock_text.starts_with("Deleting parameter failed: ")
    });
}

/// Expected-failure, issue 2484: a set off the declared step is refused by
/// both; nano-ros reports it as out of range, stock as not a valid step.
#[test]
fn off_step_refusal_reason_compared_with_stock() {
    let pair = param_pair();
    params_discovered(&pair);
    let c = pair.ask("param set {node} stepped 7");
    hold_gap(&c, "the reason an off-step parameter set is refused", |c| {
        c.nano_text.trim() == "Setting parameter failed: Value out of range"
            && c.stock_text.contains("is not a valid step")
    });
}

// ── the service, action and pubsub pairs ────────────────────────────────────

/// Matches stock: `ros2 service list -t` for a node serving one service.
///
/// The stock node's six parameter services are set aside: a nano-ros node has
/// none by default, which is a design decision awaiting a ruling (phase-556
/// W13), not a gap this test measures.
#[test]
fn service_list_with_types_answers_as_stock_does() {
    let nano_bin = build_native_service_server().require("rust workspace linux_service_server");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");

    let mut nano = GraphSide::new(Implementation::NanoRos, "/add_server");
    nano.spawn_nano_image(nano_bin, "service-server");
    nano.await_node();
    let mut stock = GraphSide::new(Implementation::Stock, "/add_two_ints_server");
    stock.spawn_stock(
        "ros2 run demo_nodes_cpp add_two_ints_server",
        "stock add_two_ints_server",
    );
    stock.await_node();
    let pair = Pair::new(nano, stock);

    pair.ask_until("service list -t", SETTLE, |a| {
        has_line(a, "/add_two_ints [")
    })
    .filtered(|l| !is_parameter_service_line(l))
    .assert_same("the services of a node serving /add_two_ints");
}

fn action_pair() -> Pair {
    let nano_bin = build_native_action_server().require("rust workspace linux_action_server");
    require_zenohd();
    require_ros2();
    require_stock_package("examples_rclcpp_minimal_action_server");

    let mut nano = GraphSide::new(Implementation::NanoRos, "/fibonacci_server");
    nano.spawn_nano_image(nano_bin, "action-server");
    nano.await_node();
    let mut stock = GraphSide::new(Implementation::Stock, "/minimal_action_server");
    stock.spawn_stock(
        "ros2 run examples_rclcpp_minimal_action_server action_server_member_functions",
        "stock minimal_action_server",
    );
    stock.await_node();
    Pair::new(nano, stock)
}

/// Matches stock: `ros2 action list -t` and `ros2 action info -t` for a node
/// serving `/fibonacci`.
#[test]
fn action_list_and_info_answer_as_stock_does() {
    let pair = action_pair();
    pair.ask_until("action list -t", SETTLE, |a| has_line(a, "/fibonacci ["))
        .assert_same("the actions of a node serving /fibonacci");
    pair.ask_until("action info -t /fibonacci", SETTLE, |a| {
        has_line(a, "Action servers: 1")
    })
    .assert_same("the servers of /fibonacci");
}

/// Matches stock: the hidden services and topics an action server brings,
/// with their types.
#[test]
fn hidden_action_services_and_topics_answer_as_stock_does() {
    let pair = action_pair();
    let action_part = || only_lines_containing(&["/fibonacci/_action/"]);
    pair.ask_until("service list -t --include-hidden-services", SETTLE, |a| {
        has_line(a, "/fibonacci/_action/send_goal [")
    })
    .filtered(action_part())
    .assert_same("the hidden services of /fibonacci");
    pair.ask_until("topic list -t --include-hidden-topics", SETTLE, |a| {
        has_line(a, "/fibonacci/_action/status [")
    })
    .filtered(action_part())
    .assert_same("the hidden topics of /fibonacci");
}

/// Matches stock: the QoS `ros2 topic info -v` reports for a default-QoS
/// publisher. History depth is set aside: the two examples declare different
/// depths, which is a difference in the examples, not in nano-ros.
#[test]
fn default_qos_of_a_published_topic_reads_as_stock_does() {
    let nano_bin = build_native_talker().require("native-rust-talker");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");

    let mut nano = GraphSide::new(Implementation::NanoRos, "/talker");
    nano.spawn_nano_image(nano_bin, "talker");
    nano.await_node();
    let mut stock = GraphSide::new(Implementation::Stock, "/talker");
    stock.spawn_stock("ros2 run demo_nodes_cpp talker", "stock talker");
    stock.await_node();
    let pair = Pair::new(nano, stock);

    pair.ask_until("topic info -v /chatter", SETTLE, |a| {
        has_line(a, "Endpoint type: PUBLISHER")
    })
    .filtered(without_lines_containing(&["GID:"]))
    .mapped(|l| match l.split_once("History (Depth): ") {
        // `KEEP_LAST (10)` -> `KEEP_LAST`
        Some((head, policy)) => format!(
            "{head}History (Depth): {}",
            policy.split(" (").next().unwrap_or(policy)
        ),
        None => l.to_string(),
    })
    .assert_same("the QoS of a default-QoS publisher of /chatter");
}

/// Binds this binary's cases to its `interop::CELLS` rows.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "ros2_cli_parity_interop",
        &[
            (Linux, Rust, Zenoh, Params),
            (Linux, Rust, Zenoh, Service),
            (Linux, Rust, Zenoh, Action),
            (Linux, Rust, Zenoh, Pubsub),
        ],
    );
}
