//! phase-310 W3 — multi-edition pub/sub E2E, both directions.
//!
//! Direct same-domain cyclone between real nano-ros example nodes (host, built
//! per-edition by `just ros_editions build-e2e-fixtures <distro>`) and a stock
//! ROS 2 edition peer (in the `DockerRosEnv` container). `std_msgs/Int32` on
//! `/chatter`.
//!
//!   - nano `talker`  → `ros2 topic echo`   (nano → ROS)
//!   - `ros2 topic pub` → nano `listener`   (ROS → nano)
//!
//! Skips (never a silent pass) without the built fixtures / docker / image. Not
//! in `just ci`; run by `just ros_editions ci`.

use std::time::Duration;

use nros_tests::ros_env;

#[test]
fn nano_talker_to_ros_echo() {
    let (env, bin, domain) = ros_env::e2e_setup("talker");
    let _talker = ros_env::spawn_process(
        {
            let mut c = ros_env::nano_node_cmd(&bin, domain, &[]);
            c.env("NROS_PUB_TYPE", "int32");
            c
        },
        "nano-talker",
    )
    .expect("spawn nano talker");

    let out = env
        .echo_topic_once("/chatter", "std_msgs/msg/Int32", 45)
        .expect("ros2 topic echo");
    assert!(
        out.contains("data:"),
        "ROS echo did not receive nano-ros Int32 on /chatter:\n{out}"
    );
}

#[test]
fn ros_pub_to_nano_listener() {
    let (env, bin, domain) = ros_env::e2e_setup("listener");
    let _pub = env
        .spawn_topic_pub("/chatter", "std_msgs/msg/Int32", "{data: 42}", 5)
        .expect("spawn ros2 topic pub");

    let mut listener = ros_env::spawn_process(
        {
            let mut c = ros_env::nano_node_cmd(&bin, domain, &[]);
            c.env("NROS_SUB_TYPE", "int32");
            c
        },
        "nano-listener",
    )
    .expect("spawn nano listener");

    let out = listener
        .wait_for_output(Duration::from_secs(45))
        .unwrap_or_default();
    assert!(
        out.contains(&format!("{} [42]", nros_tests::output::LISTENER_LOG_PREFIX)),
        "nano-ros listener did not receive ROS Int32 42 on /chatter:\n{out}"
    );
}
