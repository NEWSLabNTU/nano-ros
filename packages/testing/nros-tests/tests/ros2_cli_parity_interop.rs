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
//! | pubsub | rust workspace `linux_listener` (`/listener`) | `demo_nodes_cpp listener` |
//! | service | rust workspace `linux_service_client` (`/add_client`) | `demo_nodes_cpp add_two_ints_client` |
//! | action | rust workspace `linux_action_client` (`/fibonacci_client`) | `examples_rclcpp_minimal_action_client` |
//! | qos | `bins/qos-override-pubsub`, `graph-profile` (`/qos_override`) | `fixtures/ros2-qos-peer/qos_peer.py` (rclpy, the same profile) |
//! | params (C) | `examples/linux/c/parameters` (`/c_parameters`) | `param_peer.py c_parameters c_parameters` |
//! | params (C++) | `examples/linux/cpp/parameters` (`/cpp_parameters`) | `param_peer.py cpp_parameters cpp_parameters` |
//! | params, refusal reasons | `param-two-node-talker` (`/alpha`), the C example (`/c_parameters`) | `turtlesim turtlesim_node` (rclcpp; the reasons follow rclcpp by user ruling) |
//!
//! Live peer: the host's ROS 2 (Humble) with `rmw_zenoh_cpp`. Run with
//! `just linux test-ros2-cli-parity`; cells `native-ros2-cli-parity-*` in
//! `interop::CELLS`. Nothing compiles at run time: build the fixture rows
//! the table names first (`just linux build-fixtures`).

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

// phase-556 W4 to W6 — the pairs for `node info`, `topic list -t` and the
// QoS `topic info -v` reports.
use nros_tests::{
    fixtures::{
        build_native_action_client, build_native_listener, build_native_service_client,
        build_qos_override_pubsub,
    },
    ros2_compat::PARAMETER_SERVICES,
};

// phase-556 W2 — the C and C++ parameter examples, and the pure comparison
// for answers read concurrently with a set.
use nros_tests::{
    fixtures::{build_c_parameters, build_cpp_parameters},
    ros2_compat::compare,
};

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
/// `dump` and an accepted `set`, on declared parameters. `use_sim_time` is
/// in both lists, as stock declares it on every node.
#[test]
fn declared_parameters_answer_ros2_param_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);

    pair.ask("param list {node}")
        .assert_same("the declared parameter names");
    pair.ask("param list --param-type {node}")
        .assert_same("the declared parameter names and types");
    pair.ask("param get {node} rate")
        .assert_same("the value of an integer parameter");
    pair.ask("param get {node} stepped")
        .assert_same("the value of a range-constrained parameter");
    pair.ask("param describe {node} rate stepped")
        .assert_same("the descriptors of two declared parameters");
    pair.ask("param dump {node}")
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

/// Matches stock: every node declares `use_sim_time` (bool, false), and
/// setting it true is accepted and subscribes the node's executor to `/clock`.
// Issue 2483, phase-556 W11: was the expected-failure
// ~~`use_sim_time_compared_with_stock`~~.
#[test]
fn use_sim_time_answers_ros2_param_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);

    pair.ask("param list {node}")
        .assert_same("the parameter list, `use_sim_time` included");
    pair.ask("param get {node} use_sim_time")
        .assert_same("the value of `use_sim_time`");
    pair.ask("param set {node} use_sim_time true")
        .assert_same("an accepted set of `use_sim_time`");
    pair.ask_until("param get {node} use_sim_time", SETTLE, |a| {
        has_line(a, "True")
    })
    .assert_same("the value of `use_sim_time` read back after the set");
    // The switch is real: each side now subscribes to `/clock`, one
    // subscription per side (one executor on the nano-ros side, however many
    // nodes it composes).
    pair.ask_until("topic info /clock", SETTLE, |a| {
        has_line(a, "Subscription count: 1")
    })
    .assert_same("the `/clock` subscription a true `use_sim_time` creates");
}

/// Matches stock: `GetParameters` naming an undeclared parameter is answered
/// with no value at all, and `ros2 param get` of that name exits 1.
///
/// Issue 2484 (resolved): nano-ros answered with a filler value of type 0, so
/// the CLI read the name as present and exited 0.
#[test]
fn get_parameters_reply_for_an_undeclared_name_answers_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);

    pair.ask_until(
        "service call {node}/get_parameters rcl_interfaces/srv/GetParameters '{names: [nope]}'",
        SETTLE,
        |a| has_line(a, "GetParameters_Response("),
    )
    .filtered(|l| l.contains("GetParameters_Response("))
    .assert_same("the GetParameters reply for an undeclared name");

    pair.ask("param get {node} nope")
        .assert_same("`ros2 param get` of an undeclared name, its output and exit status");
}

/// Matches stock: `DescribeParameters` naming an undeclared parameter is
/// answered with no descriptor, and `ros2 param describe` prints nothing.
///
/// Issue 2484 (resolved): nano-ros answered with a filler descriptor the CLI
/// printed as `Type: not set`.
#[test]
fn describe_parameters_reply_for_an_undeclared_name_answers_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);

    pair.ask_until(
        "service call {node}/describe_parameters rcl_interfaces/srv/DescribeParameters \
         '{names: [nope]}'",
        SETTLE,
        |a| has_line(a, "DescribeParameters_Response("),
    )
    .filtered(|l| l.contains("DescribeParameters_Response("))
    .assert_same("the DescribeParameters reply for an undeclared name");

    pair.ask_until("param describe {node} nope", SETTLE, |_| true)
        .assert_same("`ros2 param describe` of an undeclared name");
}

/// Matches stock: deleting a statically typed parameter is refused, with
/// rclcpp's reason (`turtlesim_node`'s `background_r` on the stock side).
///
/// Issue 2484 (resolved): nano-ros refused it as `Type mismatch`. The
/// reference is rclcpp by user ruling (phase-556 W2, 2026-11-19).
#[test]
fn delete_of_a_statically_typed_parameter_is_refused_as_stock_does() {
    let pair = rclcpp_reason_pair();
    refusal_compared(
        &pair,
        "param delete {node} rate",
        "param delete {node} background_r",
    )
    .assert_same("the refusal of a delete of a statically typed parameter");
}

/// Matches stock: a set off the declared step is refused, with rclcpp's
/// reason naming the parameter and the range kind.
///
/// rclcpp gives an off-step value and an out-of-range value the same reason
/// (measured against a Humble rclcpp node declaring `stepped` with 0..100 step
/// 5: `stepped 7` and `stepped 105` both read `Parameter {stepped} doesn't
/// comply with integer range.`). No shipped rclcpp node declares a step above
/// 1, so the stock side is `turtlesim_node`'s `background_r` (0..255 step 1)
/// set out of range.
///
/// Issue 2484 (resolved): nano-ros refused it as `Value out of range`.
#[test]
fn off_step_set_is_refused_as_stock_does() {
    let pair = rclcpp_reason_pair();
    refusal_compared(
        &pair,
        "param set {node} stepped 7",
        "param set {node} background_r 300",
    )
    .assert_same("the refusal of a set off the declared step");
}

// ── the service, action and pubsub pairs ────────────────────────────────────

/// Matches stock: `ros2 service list -t` for a node serving one service,
/// the six parameter services every node serves by default included.
///
/// phase-556 W13 (user ruling 2026-11-13): a nano-ros node serves the stock
/// parameter services unless its image opts out, so they are compared rather
/// than set aside.
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

    // Ready once the side lists its own service and all six parameter
    // services, so a half-discovered node is not compared.
    pair.ask_until("service list -t", SETTLE, |a| {
        has_line(a, "/add_two_ints [")
            && a.output
                .lines()
                .filter(|l| is_parameter_service_line(l))
                .count()
                >= 6
    })
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

// ── node info, topic list and QoS (phase-556 W4 to W6) ──────────────────────

/// A line of the interfaces every stock node has by default: `/rosout`,
/// `/parameter_events` (publisher and, for rclcpp, subscriber) and the six
/// parameter services. Set aside where a test measures something else;
/// `default_node_interfaces_compared_with_stock` compares them itself (a
/// nano-ros node has them all by default, phase-556 W13, but the
/// subscription).
fn is_default_node_interface_line(line: &str) -> bool {
    line.contains("/rosout: ")
        || line.contains("/parameter_events: ")
        || is_parameter_service_line(line)
}

/// A nano-ros image and a stock node, each alone on its own router.
fn role_pair(
    nano_bin: &std::path::Path,
    nano_node: &str,
    nano_env: &[(&str, &str)],
    stock_cmd: &str,
    stock_node: &str,
) -> Pair {
    let mut nano = GraphSide::new(Implementation::NanoRos, nano_node);
    nano.spawn_nano_image_with_env(nano_bin, nano_node, nano_env);
    nano.await_node();
    let mut stock = GraphSide::new(Implementation::Stock, stock_node);
    stock.spawn_stock(stock_cmd, stock_node);
    stock.await_node();
    Pair::new(nano, stock)
}

fn talker_pair() -> Pair {
    let nano_bin = build_native_talker().require("native-rust-talker");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");
    role_pair(
        nano_bin,
        "/talker",
        &[],
        "ros2 run demo_nodes_cpp talker",
        "/talker",
    )
}

fn listener_pair() -> Pair {
    let nano_bin = build_native_listener().require("rust workspace linux_listener");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");
    role_pair(
        nano_bin,
        "/listener",
        &[],
        "ros2 run demo_nodes_cpp listener",
        "/listener",
    )
}

/// `ros2 node info {node}`, asked until the side lists `own` (an interface
/// line of the node's own), with its blank lines dropped (see
/// [`node_info_lines`]).
fn node_info(pair: &Pair, own: &str) -> Compared {
    pair.ask_until("node info {node}", SETTLE, |a| has_line(a, own))
        .filtered(node_info_lines)
}

/// `ros2 node info` prints a blank line under a section with no entries, so
/// setting a section's entries aside would leave a difference of one blank
/// line; the section headers alone carry the layout.
fn node_info_lines(line: &str) -> bool {
    !line.trim().is_empty()
}

/// [`node_info`] with the default node interfaces set aside.
fn node_info_without_defaults(pair: &Pair, own: &str) -> Compared {
    node_info(pair, own).filtered(|l| !is_default_node_interface_line(l))
}

/// Matches stock: `ros2 node info` of a node publishing `/chatter`.
#[test]
fn node_info_of_a_publisher_answers_as_stock_does() {
    let pair = talker_pair();
    node_info_without_defaults(&pair, "/chatter: ")
        .assert_same("the interfaces of a node publishing /chatter");
}

/// Matches stock: `ros2 node info` of a node subscribing to `/chatter`.
#[test]
fn node_info_of_a_subscriber_answers_as_stock_does() {
    let pair = listener_pair();
    node_info_without_defaults(&pair, "/chatter: ")
        .assert_same("the interfaces of a node subscribing to /chatter");
}

/// Matches stock: `ros2 node info` of a node serving `/add_two_ints`.
#[test]
fn node_info_of_a_service_server_answers_as_stock_does() {
    let nano_bin = build_native_service_server().require("rust workspace linux_service_server");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");
    let pair = role_pair(
        nano_bin,
        "/add_server",
        &[],
        "ros2 run demo_nodes_cpp add_two_ints_server",
        "/add_two_ints_server",
    );
    node_info_without_defaults(&pair, "/add_two_ints: ")
        .assert_same("the interfaces of a node serving /add_two_ints");
}

/// Matches stock: `ros2 node info` of a node with a client of
/// `/add_two_ints`. Neither side has a server, so both clients keep waiting
/// for one and stay in the graph.
#[test]
fn node_info_of_a_service_client_answers_as_stock_does() {
    let nano_bin = build_native_service_client().require("rust workspace linux_service_client");
    require_zenohd();
    require_ros2();
    require_stock_package("demo_nodes_cpp");
    let pair = role_pair(
        nano_bin,
        "/add_client",
        &[],
        "ros2 run demo_nodes_cpp add_two_ints_client",
        "/add_two_ints_client",
    );
    node_info_without_defaults(&pair, "/add_two_ints: ")
        .assert_same("the interfaces of a node calling /add_two_ints");
}

/// Matches stock: `ros2 node info` of a node serving the `/fibonacci` action.
#[test]
fn node_info_of_an_action_server_answers_as_stock_does() {
    let pair = action_pair();
    node_info_without_defaults(&pair, "/fibonacci: ")
        .assert_same("the interfaces of a node serving the /fibonacci action");
}

/// Matches stock: `ros2 node info` of a node with a client of the
/// `/fibonacci` action. Neither side has a server, so both clients keep
/// waiting for one and stay in the graph.
#[test]
fn node_info_of_an_action_client_answers_as_stock_does() {
    let nano_bin = build_native_action_client().require("rust workspace linux_action_client");
    require_zenohd();
    require_ros2();
    require_stock_package("examples_rclcpp_minimal_action_client");
    let pair = role_pair(
        nano_bin,
        "/fibonacci_client",
        &[],
        "ros2 run examples_rclcpp_minimal_action_client action_client_member_functions",
        "/minimal_action_client",
    );
    node_info_without_defaults(&pair, "/fibonacci: ")
        .assert_same("the interfaces of a node calling the /fibonacci action");
}

/// A stock node publishes `/rosout` and `/parameter_events`, serves the six
/// parameter services and (rclcpp) subscribes to `/parameter_events`. A
/// nano-ros node has the same by default (phase-556 W13), all but the
/// subscription: held in expected-failure form, the one line it lacks.
// The subscriber is issue 2483 (phase-556 W11). RFC-0036 "Graph interfaces a
// node has by default".
#[test]
fn default_node_interfaces_compared_with_stock() {
    let pair = listener_pair();
    // The stock side is read once it lists every default interface, so a
    // half-discovered node is not compared.
    let stock_ready = |a: &CliAnswer| {
        has_line(a, "/chatter: ")
            && has_line(a, "/rosout: ")
            && a.output.matches("/parameter_events: ").count() == 2
            && PARAMETER_SERVICES
                .iter()
                .all(|(s, _)| has_line(a, &format!("/{s}: ")))
    };
    // The nano-ros side the same, less the subscription it does not make.
    let nano_ready = |a: &CliAnswer| {
        has_line(a, "/chatter: ")
            && has_line(a, "/rosout: ")
            && has_line(a, "/parameter_events: ")
            && PARAMETER_SERVICES
                .iter()
                .all(|(s, _)| has_line(a, &format!("/{s}: ")))
    };
    let c = pair
        .ask_each("node info {node}", SETTLE, nano_ready, stock_ready)
        .filtered(node_info_lines);
    hold_gap(&c, "the interfaces a node has by default", |c| {
        let Some(d) = c.diff() else { return false };
        // Only the stock side's `/parameter_events` subscription: the
        // publishers and the parameter services match.
        changed_lines(&d) == ["+     /parameter_events: rcl_interfaces/msg/ParameterEvent"]
    });
}

/// Matches stock: `ros2 topic list -t` on a graph holding a node publishing
/// `/chatter`, and on one holding a node subscribing to it.
///
/// `/rosout` and `/parameter_events` are listed on both sides, but not because
/// both nodes publish them: the `ros2` CLI's own node does, on either side. A
/// node's own share of them is what `ros2 node info` shows, held by
/// `default_node_interfaces_compared_with_stock`.
#[test]
fn topic_list_with_types_answers_as_stock_does() {
    for (pair, role) in [
        (talker_pair(), "publishing"),
        (listener_pair(), "subscribing to"),
    ] {
        pair.ask_until("topic list -t", SETTLE, |a| has_line(a, "/chatter ["))
            .assert_same(&format!(
                "the topics of a graph with a node {role} /chatter"
            ));
    }
}

/// `History (Depth): KEEP_LAST (10)` -> `History (Depth): KEEP_LAST`.
fn without_history_depth(line: &str) -> String {
    match line.split_once("History (Depth): ") {
        Some((head, policy)) => format!(
            "{head}History (Depth): {}",
            policy.split(" (").next().unwrap_or(policy)
        ),
        None => line.to_string(),
    }
}

/// Matches stock, but for the depth: the QoS `ros2 topic info -v` reports for
/// a default-QoS subscription.
///
/// By design, not a gap: a zenoh subscription keeps the depth its receive
/// ring holds (4 by default, a build knob) and says so, where stock keeps
/// the 10 the default profile asks for. RFC-0036 "Reduced / omitted surface".
#[test]
fn default_qos_of_a_subscription_reads_as_stock_does() {
    let pair = listener_pair();
    pair.ask_until("topic info -v /chatter", SETTLE, |a| {
        has_line(a, "Endpoint type: SUBSCRIPTION")
    })
    .filtered(without_lines_containing(&["GID:"]))
    .mapped(without_history_depth)
    .assert_same("the QoS of a default-QoS subscription to /chatter");
}

/// A `/chatter` endpoint whose QoS has every policy off its default (best
/// effort, transient local, keep last 1 or 3, a 1.5 s deadline, a 2 s lifespan,
/// manual-by-topic liveliness with a 3 s lease), on a nano-ros node and on a
/// stock rclpy node.
fn non_default_qos_pair(nano_role: &str, stock_role: &str) -> Pair {
    let nano_bin = build_qos_override_pubsub().require("qos-override-pubsub");
    require_zenohd();
    require_ros2();
    let peer = nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/ros2-qos-peer/qos_peer.py");
    role_pair(
        nano_bin,
        "/qos_override",
        &[
            ("NROS_QOS_ROLE", nano_role),
            ("NROS_QOS_OVERRIDE", "graph-profile"),
        ],
        &format!("python3 -u {} {stock_role}", peer.display()),
        "/qos_override",
    )
}

/// Matches stock: every QoS policy `ros2 topic info -v` reports for a
/// publisher with non-default QoS.
///
/// Until phase-556 W6 the zenoh graph token left the deadline, the lifespan
/// and the liveliness unstated, so a peer read them as infinite and AUTOMATIC
/// (`nros-rmw-zenoh`, `QosKeyExpr::to_qos_string`).
#[test]
fn non_default_qos_of_a_publisher_reads_as_stock_does() {
    let pair = non_default_qos_pair("talker", "publisher");
    pair.ask_until("topic info -v /chatter", SETTLE, |a| {
        has_line(a, "Endpoint type: PUBLISHER")
    })
    .filtered(without_lines_containing(&["GID:"]))
    .assert_same("the QoS of a publisher of /chatter with non-default QoS");
}

/// Matches stock: every QoS policy `ros2 topic info -v` reports for a
/// subscription with non-default QoS. See the publisher's test.
#[test]
fn non_default_qos_of_a_subscription_reads_as_stock_does() {
    let pair = non_default_qos_pair("listener", "subscription");
    pair.ask_until("topic info -v /chatter", SETTLE, |a| {
        has_line(a, "Endpoint type: SUBSCRIPTION")
    })
    .filtered(without_lines_containing(&["GID:"]))
    .assert_same("the QoS of a subscription to /chatter with non-default QoS");
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
            (Linux, Rust, Zenoh, Qos),
            (Linux, C, Zenoh, Params),
            (Linux, Cpp, Zenoh, Params),
        ],
    );
}

// ── parameter refusals, /parameter_events, load, C and C++ (phase-556 W2) ───

/// The stock rclcpp node the refusal reasons are compared against:
/// `turtlesim_node`, a shipped rclcpp node with statically typed integer
/// parameters in a range (`background_r`, 0..255 step 1) and read-only ones
/// (`qos_overrides./parameter_events.publisher.*`). No display: Qt draws
/// offscreen.
///
/// The reference is rclcpp by user ruling (phase-556 W2, 2026-11-19); the
/// other parameter tests keep the rclpy peer, which declares the same names
/// as the nano-ros node.
fn rclcpp_reference_side() -> GraphSide {
    require_stock_package("turtlesim");
    let mut stock = GraphSide::new(Implementation::Stock, "/turtlesim");
    stock.spawn_stock(
        "env QT_QPA_PLATFORM=offscreen ros2 run turtlesim turtlesim_node",
        "stock turtlesim_node (rclcpp)",
    );
    stock.await_node();
    stock
}

/// Ready once the side's `param list` shows `name`.
fn param_listed(side: &GraphSide, name: &str) {
    let a = side.query_until("param list {node}", SETTLE, |a| has_line(a, name));
    assert!(
        has_line(&a, name),
        "the {} node's parameter services never listed `{name}`:\n{a}",
        side.implementation
    );
}

/// nano-ros: `param-two-node-talker`'s `/alpha`. Stock: `turtlesim_node`.
fn rclcpp_reason_pair() -> Pair {
    let nano_bin = build_native_param_two_node_talker().require("param-two-node-talker");
    require_zenohd();
    require_ros2();
    let mut nano = GraphSide::new(Implementation::NanoRos, "/alpha");
    nano.spawn_nano_image(nano_bin, "param-two-node-talker");
    nano.await_node();
    let stock = rclcpp_reference_side();
    param_listed(&nano, "stepped");
    param_listed(&stock, "background_r");
    Pair::new(nano, stock)
}

/// Ask each side its own refusal query (the stock node declares other
/// names) and compare the answers, with every parameter name the two queries
/// name read as `<param>` in the `{name}` and `'name'` forms the reasons use.
fn refusal_compared(pair: &Pair, nano_args: &str, stock_args: &str) -> Compared {
    let nano = pair
        .nano
        .query_until(nano_args, SETTLE, |a| !a.output.trim().is_empty());
    let stock = pair
        .stock
        .query_until(stock_args, SETTLE, |a| !a.output.trim().is_empty());
    // A refusal on both sides, so the comparison is about its reason.
    for a in [&nano, &stock] {
        assert!(
            has_line(a, "failed: "),
            "`ros2 {}` was not refused:\n{a}",
            a.args
        );
    }
    let names: Vec<String> = [nano_args, stock_args]
        .iter()
        .filter_map(|q| q.split_whitespace().nth(3).map(str::to_string))
        .collect();
    compare(
        &format!("{nano_args}` / `ros2 {stock_args}"),
        nano,
        &pair.nano.node,
        stock,
        &pair.stock.node,
    )
    .mapped(move |l| {
        names.iter().fold(l.to_string(), |l, n| {
            l.replace(&format!("{{{n}}}"), "{<param>}")
                .replace(&format!("'{n}'"), "'<param>'")
        })
    })
}

/// Matches stock: `ros2 param set` of a name the node never declared is
/// refused with rclcpp's reason, which names the parameter, and the same exit
/// status.
///
/// Issue 2731 (resolved): nano-ros gave rclrs's `Parameter was not declared
/// and undeclared parameters are not allowed`; the user ruled for rclcpp's
/// wording (2026-11-19).
#[test]
fn set_of_an_undeclared_name_is_refused_as_stock_does() {
    let pair = rclcpp_reason_pair();
    refusal_compared(&pair, "param set {node} nope 1", "param set {node} nope 1")
        .assert_same("the refusal of a set of an undeclared parameter");
}

/// Matches stock: `ros2 param set` of a declared integer parameter with a
/// string value is refused with rclcpp's reason, naming the parameter and
/// both types, and the same exit status; the value is unchanged.
#[test]
fn set_of_the_wrong_type_is_refused_as_stock_does() {
    let pair = rclcpp_reason_pair();
    refusal_compared(
        &pair,
        "param set {node} rate hello",
        "param set {node} background_r hello",
    )
    .assert_same("the refusal of a set of an integer parameter to a string");
    let after = pair
        .nano
        .query("param get {node} rate")
        .unwrap_or_else(|e| panic!("nano-ros side: {e}"));
    assert!(
        has_line(&after, "Integer value is: 10"),
        "a refused set changed the value of `rate`:\n{after}"
    );
}

/// Matches stock: `ros2 param set` of a read-only parameter is refused with
/// rclcpp's reason, naming the parameter. nano-ros: the C parameter example's
/// `topic_name` (read-only through `rclc_set_parameter_read_only`). Stock:
/// `turtlesim_node`'s `qos_overrides./parameter_events.publisher.depth`,
/// which rclcpp declares read-only on every node.
#[test]
fn set_of_a_read_only_parameter_is_refused_as_stock_does() {
    let nano_bin = build_c_parameters().require("c-parameters");
    require_zenohd();
    require_ros2();
    let mut nano = GraphSide::new(Implementation::NanoRos, "/c_parameters");
    nano.spawn_nano_image_with_env(nano_bin, "c-parameters", &[("NROS_SPIN_MS", "300000")]);
    nano.await_node();
    let stock = rclcpp_reference_side();
    param_listed(&nano, "topic_name");
    param_listed(&stock, "qos_overrides./parameter_events.publisher.depth");
    let pair = Pair::new(nano, stock);
    refusal_compared(
        &pair,
        "param set {node} topic_name /other",
        "param set {node} qos_overrides./parameter_events.publisher.depth 5",
    )
    .assert_same("the refusal of a set of a read-only parameter");
}

/// The `/parameter_events` message a side's node publishes for `set`, as
/// `ros2 topic echo --once` prints it. The echo is started first and the set
/// is sent once the echo's subscription is in the side's graph, so the one
/// message it prints is the set's. Only the node's own events are echoed: the
/// `ros2 param set` CLI is a node too, and announces its own parameters.
fn parameter_event_for(side: &GraphSide, set: &str) -> CliAnswer {
    let echo = "topic echo --once --filter \"m.node == '{node}'\" /parameter_events \
                rcl_interfaces/msg/ParameterEvent";
    let subscriptions = |a: &CliAnswer| {
        a.output
            .lines()
            .find_map(|l| l.strip_prefix("Subscription count: "))
            .and_then(|n| n.trim().parse::<u32>().ok())
    };
    let info = "topic info /parameter_events";
    let before = side.query_until(info, SETTLE, |a| subscriptions(a).is_some());
    let before = subscriptions(&before)
        .unwrap_or_else(|| panic!("`ros2 {info}` printed no subscription count:\n{before}"));
    std::thread::scope(|s| {
        let echoed = s.spawn(|| {
            side.query(echo)
                .unwrap_or_else(|e| panic!("{} side: {e}", side.implementation))
        });
        let subscribed = side.query_until(info, SETTLE, |a| {
            subscriptions(a).is_some_and(|n| n > before)
        });
        assert!(
            subscriptions(&subscribed).is_some_and(|n| n > before),
            "the {} side's `ros2 topic echo /parameter_events` never subscribed:\n{subscribed}",
            side.implementation
        );
        std::thread::sleep(Duration::from_secs(1));
        let set_answer = side
            .query(set)
            .unwrap_or_else(|e| panic!("{} side: {e}", side.implementation));
        let event = echoed.join().expect("the echo thread panicked");
        assert!(
            has_line(&event, "changed_parameters:") || has_line(&event, "new_parameters:"),
            "the {} side's node published no /parameter_events message for \
             `ros2 {set}`:\n{event}\nThe set answered:\n{set_answer}",
            side.implementation
        );
        event
    })
}

/// Matches stock: the `/parameter_events` message a node publishes for an
/// accepted `ros2 param set`: the node's name and the new, changed and deleted
/// parameter lists, field by field. The stamp is set aside.
#[test]
fn parameter_event_for_a_set_reads_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);
    let set = "param set {node} rate 13";
    let nano = parameter_event_for(&pair.nano, set);
    let stock = parameter_event_for(&pair.stock, set);
    compare(
        "topic echo --once /parameter_events",
        nano,
        &pair.nano.node,
        stock,
        &pair.stock.node,
    )
    .filtered(without_lines_containing(&["stamp:", "sec:", "nanosec:"]))
    .assert_same("the /parameter_events message for an accepted set");
}

/// Matches stock: `ros2 param load` of a parameter file naming two declared
/// parameters: its report, and the values read back.
#[test]
fn param_load_answers_as_stock_does() {
    let pair = param_pair();
    params_discovered(&pair);
    let dir = tempfile::tempdir().expect("create a dir for the parameter file");
    let file = dir.path().join("params.yaml");
    std::fs::write(
        &file,
        "/**:\n  ros__parameters:\n    rate: 12\n    stepped: 15\n",
    )
    .expect("write the parameter file");

    pair.ask(&format!("param load {{node}} {}", file.display()))
        .assert_same("`ros2 param load` of a file naming two declared parameters");
    pair.ask_until("param get {node} rate", SETTLE, |a| has_line(a, "12"))
        .assert_same("the value of `rate` after the load");
    pair.ask_until("param get {node} stepped", SETTLE, |a| has_line(a, "15"))
        .assert_same("the value of `stepped` after the load");
}

/// nano-ros: a C or C++ parameter example, left spinning. Stock: the rclpy
/// peer declaring what that example holds once its own local roundtrip is
/// done (`param_peer.py`'s `profile`).
fn example_param_pair(nano_bin: &std::path::Path, node: &str, profile: &str) -> Pair {
    require_zenohd();
    require_ros2();
    let peer = nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/ros2-param-peer/param_peer.py");
    let bare = node.trim_start_matches('/');
    let pair = role_pair(
        nano_bin,
        node,
        // The example answers its parameter services only while it spins.
        &[("NROS_SPIN_MS", "300000")],
        &format!("python3 -u {} {bare} {profile}", peer.display()),
        node,
    );
    let c = pair.ask_until("param list {node}", SETTLE, |a| {
        a.output.lines().filter(|l| l.starts_with("  ")).count() >= 4
    });
    for (side, a) in [("nano-ros", &c.nano), ("stock", &c.stock)] {
        assert!(
            a.output.lines().filter(|l| l.starts_with("  ")).count() >= 4,
            "the {side} node {node}'s parameter services were never discovered:\n{a}"
        );
    }
    pair
}

/// `param list`, `list --param-type`, `get` and `describe` of each name,
/// an accepted `set` and its read-back.
fn assert_core_param_queries_match(pair: &Pair, names: &[&str], set: (&str, &str)) {
    let no_sim_time = || without_lines_containing(&["use_sim_time"]);
    pair.ask("param list {node}")
        .filtered(no_sim_time())
        .assert_same("the declared parameter names");
    pair.ask("param list --param-type {node}")
        .filtered(no_sim_time())
        .assert_same("the declared parameter names and types");
    for name in names {
        pair.ask(&format!("param get {{node}} {name}"))
            .assert_same(&format!("the value of `{name}`"));
        pair.ask(&format!("param describe {{node}} {name}"))
            .assert_same(&format!("the descriptor of `{name}`"));
    }
    let (name, value) = set;
    pair.ask(&format!("param set {{node}} {name} {value}"))
        .assert_same(&format!("an accepted set of `{name}`"));
    pair.ask_until(&format!("param get {{node}} {name}"), SETTLE, |a| {
        has_line(a, value)
    })
    .assert_same(&format!("the value of `{name}` read back after the set"));
}

/// Matches stock: `ros2 param list`, `get`, `describe` and an accepted `set`
/// against the C parameter example (rclc's parameter server).
#[test]
fn c_parameters_answer_ros2_param_as_stock_does() {
    let nano_bin = build_c_parameters().require("c-parameters");
    let pair = example_param_pair(nano_bin, "/c_parameters", "c_parameters");
    assert_core_param_queries_match(
        &pair,
        &["verbose", "publish_rate_hz", "scale_factor", "topic_name"],
        ("scale_factor", "2.5"),
    );
}

/// Matches stock: `ros2 param list`, `get`, `describe` and an accepted `set`
/// against the C++ parameter example.
#[test]
fn cpp_parameters_answer_ros2_param_as_stock_does() {
    let nano_bin = build_cpp_parameters().require("cpp-parameters");
    let pair = example_param_pair(nano_bin, "/cpp_parameters", "cpp_parameters");
    assert_core_param_queries_match(
        &pair,
        &[
            "verbose",
            "max_iters",
            "ctrl_period",
            "frame_id",
            "mpc_weights",
        ],
        ("ctrl_period", "0.25"),
    );
}
