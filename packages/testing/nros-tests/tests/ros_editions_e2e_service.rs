//! phase-310 W4 — multi-edition service E2E, both directions.
//!
//! Direct same-domain cyclone, `example_interfaces/srv/AddTwoInts` on
//! `/add_two_ints`, between real nano-ros example nodes (host) and a ROS 2
//! edition peer (in the `DockerRosEnv` container):
//!
//!   - nano `service-client` → rclpy `add_two_ints` server  (nano → ROS)
//!   - `ros2 service call`   → nano `service-server`         (ROS → nano)
//!
//! Skips without the built fixtures / docker / image. Run by `just ros_editions ci`.

use std::time::Duration;

use nros_tests::ros_env;

#[test]
fn nano_service_client_to_ros_server() {
    let (env, bin, domain) = ros_env::e2e_setup("service-client");

    // rclpy AddTwoInts server in the container, same domain.
    let _server = env.spawn_add_two_ints_server().expect("spawn rclpy server");

    // nano-ros one-shot client with default summands 2 + 3 = 5.
    let mut client =
        ros_env::spawn_process(ros_env::nano_node_cmd(&bin, domain, &[]), "nano-srv-client")
            .expect("spawn nano service-client");

    let out = client
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains("Result of add_two_ints: 5"),
        "nano-ros service-client did not get sum 5 from the ROS server:\n{out}"
    );
}

#[test]
fn ros_client_to_nano_service_server() {
    let (env, bin, domain) = ros_env::e2e_setup("service-server");

    // nano-ros AddTwoInts server (host), same domain.
    let _server =
        ros_env::spawn_process(ros_env::nano_node_cmd(&bin, domain, &[]), "nano-srv-server")
            .expect("spawn nano service-server");

    // ros2 client calls 5 + 3 = 8; the reply shows in the call output.
    let out = env
        .service_call_add_two_ints(5, 3, 40)
        .expect("ros2 service call");
    assert!(
        out.contains("sum=8"),
        "ROS service call did not get sum 8 from the nano-ros server:\n{out}"
    );
}
