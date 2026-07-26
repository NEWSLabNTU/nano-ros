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

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use nros_tests::ros_env::{self, DockerRosEnv, Middleware, RosEnv};

/// The per-edition cyclone build of native example `name`.
fn example_bin(name: &str, edition: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../examples/native/rust")
        .join(name)
        .join(format!("target-ros-edition-{edition}/debug/{name}"))
}

/// A nano-ros example node on `domain`, cyclone RMW. Wrapped in `bash -c 'exec …
/// 2>&1'` so the node's `env_logger` output (which goes to STDERR) merges into
/// the captured stdout — a `RosPeer` reads stdout, and the receive-side markers
/// (`I heard: […]`) are logged, not printed.
fn nano_node(bin: &Path, domain: u8) -> Command {
    let mut c = Command::new("bash");
    c.arg("-c")
        .arg(format!("exec {} 2>&1", bin.display()))
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp")
        .env("RUST_LOG", "info");
    c
}

/// Guard: the edition ROS peer usable + the example fixture built, else skip.
fn setup(name: &str) -> Option<(DockerRosEnv, PathBuf, u8)> {
    let ed = ros_env::test_edition();
    let bin = example_bin(name, &ed);
    if !bin.is_file() {
        nros_tests::skip!(
            "example `{name}` not built for {ed} — run `just ros_editions build-e2e-fixtures {ed}`"
        );
    }
    let domain = nros_tests::unique_ros_domain_id();
    let env = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: domain });
    if !env.available() {
        nros_tests::skip!(
            "{ed} image not built or docker absent — run `just ros_editions image {ed}`"
        );
    }
    Some((env, bin, domain))
}

#[test]
fn nano_talker_to_ros_echo() {
    let Some((env, bin, domain)) = setup("talker") else {
        return;
    };
    let _talker = ros_env::spawn_process(
        {
            let mut c = nano_node(&bin, domain);
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
    let Some((env, bin, domain)) = setup("listener") else {
        return;
    };
    let _pub = env
        .spawn_topic_pub("/chatter", "std_msgs/msg/Int32", "{data: 42}", 5)
        .expect("spawn ros2 topic pub");

    let mut listener = ros_env::spawn_process(
        {
            let mut c = nano_node(&bin, domain);
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
        out.contains("I heard: [42]"),
        "nano-ros listener did not receive ROS Int32 42 on /chatter:\n{out}"
    );
}
