//! phase-310 W5 — multi-edition action E2E, both directions.
//!
//! Direct same-domain cyclone, `example_interfaces/action/Fibonacci` on
//! `/fibonacci`, between real nano-ros example nodes (host) and a ROS 2 edition
//! peer (in the `DockerRosEnv` container):
//!
//!   - nano `action-client` → rclpy `fibonacci` server   (nano → ROS)
//!   - `ros2 action send_goal` → nano `action-server`     (ROS → nano)
//!
//! Both use goal order 10, whose result sequence contains 34 (a distinctive
//! Fibonacci value used to confirm the result crossed). Skips without the built
//! fixtures / docker / image. Run by `just ros_editions ci`.

use std::time::Duration;

use nros_tests::ros_env;

#[test]
fn nano_action_client_to_ros_server() {
    let (env, bin, domain) = ros_env::e2e_setup("action-client");

    // rclpy Fibonacci server in the container, same domain.
    let _server = env
        .spawn_fibonacci_server()
        .expect("spawn rclpy action server");

    // nano-ros client sends order-10 goal, streams feedback, logs the result.
    let mut client = ros_env::spawn_process(
        ros_env::nano_node_cmd(&bin, domain, &[]),
        "nano-action-client",
    )
    .expect("spawn nano action-client");

    let out = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    assert!(
        out.contains("Result received:") && out.contains("34"),
        "nano-ros action-client did not get the Fibonacci result from the ROS server:\n{out}"
    );
}

#[test]
fn ros_client_to_nano_action_server() {
    let (env, bin, domain) = ros_env::e2e_setup("action-server");

    // nano-ros Fibonacci server (host), same domain.
    let _server = ros_env::spawn_process(
        ros_env::nano_node_cmd(&bin, domain, &[]),
        "nano-action-server",
    )
    .expect("spawn nano action-server");

    // ros2 client sends an order-10 goal; the result sequence shows in the output.
    let out = env
        .action_send_goal_fibonacci(10, 55)
        .expect("ros2 action send_goal");
    assert!(
        out.contains("34"),
        "ROS action client did not get the Fibonacci result from the nano-ros server:\n{out}"
    );
}
