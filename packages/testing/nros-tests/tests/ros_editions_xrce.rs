//! phase-311 W6 — multi-edition XRCE interop, both directions.
//!
//! nano-ros `rmw-xrce` example nodes (host, built per-edition by
//! `just ros_editions build-e2e-fixtures <distro> xrce`) reach a stock ROS 2
//! edition peer through a **micro-XRCE Agent** (host, from the nros SDK store):
//!
//!   nano-ros node ──XRCE/UDP──▶ Agent ──Fast-DDS──▶ `rmw_fastrtps_cpp` peer
//!
//! The Agent bridges the nano-ros XRCE client onto a Fast-DDS domain; the ROS
//! peer runs `rmw_fastrtps_cpp` in the `DockerRosEnv` container (`--network
//! host`, same domain). Covers pub/sub, service, and action, both directions.
//!
//! Skips (never a silent pass) without the xrce fixtures / the Agent / docker /
//! the image. Not in `just ci`; run by `just ros_editions ci <distro>` with
//! `NROS_RMW=xrce`.

use std::time::Duration;

use nros_tests::ros_env::{self, Rmw};

/// The nano-ros node command for the xrce lane: `bin` over the XRCE RMW, agent
/// at `127.0.0.1:<port>`, DDS side on `domain`.
fn nano_xrce(
    bin: &std::path::Path,
    domain: u8,
    port: u16,
    extra: &[&str],
) -> std::process::Command {
    let locator = format!("127.0.0.1:{port}");
    ros_env::nano_node_cmd_rmw(bin, Rmw::Xrce, domain, &locator, extra)
}

// ---- pub/sub (std_msgs/Int32 on /chatter) ----------------------------------

#[test]
fn nano_talker_to_ros_echo_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("talker");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _talker = ros_env::spawn_process(
        {
            let mut c = nano_xrce(&bin, domain, port, &[]);
            c.env("NROS_PUB_TYPE", "int32");
            c
        },
        "nano-talker-xrce",
    )
    .expect("spawn nano talker");

    let out = env
        .echo_topic_once("/chatter", "std_msgs/msg/Int32", 45)
        .expect("ros2 topic echo");
    assert!(
        out.contains("data:"),
        "ROS echo did not receive nano-ros Int32 through the XRCE Agent:\n{out}"
    );
}

#[test]
fn ros_pub_to_nano_listener_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("listener");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _pub = env
        .spawn_topic_pub("/chatter", "std_msgs/msg/Int32", "{data: 42}", 5)
        .expect("spawn ros2 topic pub");

    let mut listener = ros_env::spawn_process(
        {
            let mut c = nano_xrce(&bin, domain, port, &[]);
            c.env("NROS_SUB_TYPE", "int32");
            c
        },
        "nano-listener-xrce",
    )
    .expect("spawn nano listener");

    let out = listener
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains("I heard: [42]"),
        "nano-ros listener did not receive ROS Int32 42 through the XRCE Agent:\n{out}"
    );
}

// ---- service (example_interfaces/AddTwoInts on /add_two_ints) --------------

#[test]
fn nano_service_client_to_ros_server_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("service-client");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _server = env.spawn_add_two_ints_server().expect("spawn rclpy server");

    let mut client =
        ros_env::spawn_process(nano_xrce(&bin, domain, port, &[]), "nano-srv-client-xrce")
            .expect("spawn nano service-client");

    let out = client
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains("Result of add_two_ints: 5"),
        "nano-ros service-client did not get sum 5 through the XRCE Agent:\n{out}"
    );
}

#[test]
fn ros_client_to_nano_service_server_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("service-server");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _server =
        ros_env::spawn_process(nano_xrce(&bin, domain, port, &[]), "nano-srv-server-xrce")
            .expect("spawn nano service-server");

    let out = env
        .service_call_add_two_ints(5, 3, 40)
        .expect("ros2 service call");
    assert!(
        out.contains("sum=8"),
        "ROS service call did not get sum 8 through the XRCE Agent:\n{out}"
    );
}

// ---- action (example_interfaces/Fibonacci on /fibonacci) -------------------

#[test]
fn nano_action_client_to_ros_server_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("action-client");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _server = env
        .spawn_fibonacci_server()
        .expect("spawn rclpy action server");

    let mut client = ros_env::spawn_process(
        nano_xrce(&bin, domain, port, &[]),
        "nano-action-client-xrce",
    )
    .expect("spawn nano action-client");

    let out = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    assert!(
        out.contains("Result received:") && out.contains("34"),
        "nano-ros action-client did not get the Fibonacci result through the XRCE Agent:\n{out}"
    );
}

#[test]
fn ros_client_to_nano_action_server_xrce() {
    let (env, bin, domain, agent, port) = ros_env::e2e_setup_xrce("action-server");
    let _agent = ros_env::spawn_xrce_agent(&agent, port, domain).expect("spawn xrce agent");

    let _server = ros_env::spawn_process(
        nano_xrce(&bin, domain, port, &[]),
        "nano-action-server-xrce",
    )
    .expect("spawn nano action-server");

    let out = env
        .action_send_goal_fibonacci(10, 55)
        .expect("ros2 action send_goal");
    assert!(
        out.contains("34"),
        "ROS action client did not get the Fibonacci result through the nano-ros XRCE server:\n{out}"
    );
}
