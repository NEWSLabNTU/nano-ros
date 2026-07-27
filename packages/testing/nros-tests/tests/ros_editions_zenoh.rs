//! phase-311 W5 — multi-edition Zenoh interop, both directions (issue #0291).
//!
//! nano-ros `rmw-zenoh` example nodes (zpico, host, built per-edition by
//! `just ros_editions build-e2e-fixtures <distro> zenoh`) interop with a stock
//! ROS 2 edition `rmw_zenoh_cpp` peer through a shared `rmw_zenohd` router:
//!
//!   nano-ros zpico node ──zenoh──▶ rmw_zenohd router ──zenoh──▶ rmw_zenoh_cpp peer
//!
//! The router + peer run in the `DockerRosEnv` container (`--network host`,
//! shared domain); the zpico node reaches the router via
//! `NROS_LOCATOR=tcp/127.0.0.1:7447`. Covers pub/sub, service, and action, both
//! directions.
//!
//! **Interop key (issue #0291):** the zenoh WIRE protocol (`0x09`) is stable
//! across 1.x, so the pinned zpico 1.7.2 interoperates with a stock jazzy
//! `rmw_zenoh_cpp` (1.11.2). What must match is the RIHS01 type-hash TAIL of the
//! keyexpr — baked by building the fixture with the `ros-<edition>` feature (the
//! iron/jazzy keyexpr branch), NOT a zenoh version bump.
//!
//! Skips (never a silent pass) without the zenoh fixtures / docker / image. Not
//! in `just ci`; run by `just ros_editions ci <distro>` with `NROS_RMW=zenoh`.

use std::time::Duration;

use nros_tests::ros_env::{self, Rmw};

/// The nano-ros node command for the zenoh lane: `bin` over the Zenoh RMW,
/// router at `locator`, domain in the keyexpr.
fn nano_zenoh(
    bin: &std::path::Path,
    domain: u8,
    locator: &str,
    extra: &[&str],
) -> std::process::Command {
    ros_env::nano_node_cmd_rmw(bin, Rmw::Zenoh, domain, locator, extra)
}

// ---- pub/sub (std_msgs/Int32 on /chatter) ----------------------------------

#[test]
fn nano_talker_to_ros_echo_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("talker");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _talker = ros_env::spawn_process(
        {
            let mut c = nano_zenoh(&bin, domain, &locator, &[]);
            c.env("NROS_PUB_TYPE", "int32");
            c
        },
        "nano-talker-zenoh",
    )
    .expect("spawn nano talker");

    let out = env
        .echo_topic_once("/chatter", "std_msgs/msg/Int32", 45)
        .expect("ros2 topic echo");
    assert!(
        out.contains("data:"),
        "ROS echo did not receive nano-ros Int32 via rmw_zenoh_cpp:\n{out}"
    );
}

#[test]
fn ros_pub_to_nano_listener_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("listener");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _pub = env
        .spawn_topic_pub("/chatter", "std_msgs/msg/Int32", "{data: 42}", 5)
        .expect("spawn ros2 topic pub");

    let mut listener = ros_env::spawn_process(
        {
            let mut c = nano_zenoh(&bin, domain, &locator, &[]);
            c.env("NROS_SUB_TYPE", "int32");
            c
        },
        "nano-listener-zenoh",
    )
    .expect("spawn nano listener");

    let out = listener
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains("I heard: [42]"),
        "nano-ros listener did not receive ROS Int32 42 via rmw_zenoh_cpp:\n{out}"
    );
}

// ---- service (example_interfaces/AddTwoInts on /add_two_ints) --------------

#[test]
fn nano_service_client_to_ros_server_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("service-client");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _server = env.spawn_add_two_ints_server().expect("spawn rclpy server");

    let mut client = ros_env::spawn_process(
        nano_zenoh(&bin, domain, &locator, &[]),
        "nano-srv-client-zenoh",
    )
    .expect("spawn nano service-client");

    let out = client
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains("Result of add_two_ints: 5"),
        "nano-ros service-client did not get sum 5 via rmw_zenoh_cpp:\n{out}"
    );
}

#[test]
fn ros_client_to_nano_service_server_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("service-server");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _server = ros_env::spawn_process(
        nano_zenoh(&bin, domain, &locator, &[]),
        "nano-srv-server-zenoh",
    )
    .expect("spawn nano service-server");

    let out = env
        .service_call_add_two_ints(5, 3, 40)
        .expect("ros2 service call");
    assert!(
        out.contains("sum=8"),
        "ROS service call did not get sum 8 via rmw_zenoh_cpp:\n{out}"
    );
}

// ---- action (example_interfaces/Fibonacci on /fibonacci) -------------------

#[test]
fn nano_action_client_to_ros_server_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("action-client");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _server = env
        .spawn_fibonacci_server()
        .expect("spawn rclpy action server");

    let mut client = ros_env::spawn_process(
        nano_zenoh(&bin, domain, &locator, &[]),
        "nano-action-client-zenoh",
    )
    .expect("spawn nano action-client");

    let out = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    assert!(
        out.contains("Result received:") && out.contains("34"),
        "nano-ros action-client did not get the Fibonacci result via rmw_zenoh_cpp:\n{out}"
    );
}

// Issue #0292 (RESOLVED) — a nano-ros zpico ACTION SERVER now interops with a
// stock `rmw_zenoh_cpp` client. Two bugs fixed: (1) all entity liveliness tokens
// shared a hardcoded id `0/11`, so the server's five entities collided and the
// action never assembled (`ros2 action list` empty); each entity now gets a
// unique per-session id. (2) the send_goal/get_result services advertised the
// ACTION hash instead of their own SERVICE hash, so a client's query keyexpr
// missed; codegen now emits `RosAction::{SEND_GOAL,GET_RESULT}_SERVICE_HASH`.
#[test]
fn ros_client_to_nano_action_server_zenoh() {
    let (env, bin, domain, locator) = ros_env::e2e_setup_zenoh("action-server");
    let _router = env.spawn_zenoh_router().expect("spawn rmw_zenohd");

    let _server = ros_env::spawn_process(
        nano_zenoh(&bin, domain, &locator, &[]),
        "nano-action-server-zenoh",
    )
    .expect("spawn nano action-server");

    let out = env
        .action_send_goal_fibonacci(10, 55)
        .expect("ros2 action send_goal");
    assert!(
        out.contains("34"),
        "ROS action client did not get the Fibonacci result from the nano-ros zenoh server:\n{out}"
    );
}
