//! phase-309 W3 — `DockerRosEnv` backend smoke test.
//!
//! Proves the docker-backed [`RosEnv`] can spawn a live ROS 2 peer in an
//! edition image the host does NOT have installed, run a one-shot command
//! against it, and observe delivery — the primitive every extra-edition lane is
//! built on. Skips cleanly (never a silent pass) when the image is not built or
//! docker is absent, matching the QEMU-lane contract.
//!
//! Run it (jazzy image must exist — `just ros_editions image jazzy`):
//!   cargo test -p nros-tests --test ros_editions_smoke -- --nocapture
//!
//! It is NOT part of `just ci` (docker + a built image are required); the
//! `just ros_editions ci` composite (W6) runs it.

use nros_tests::ros_env::{self, DockerRosEnv, Middleware, RosEnv};

/// A stock ROS 2 publisher (in the edition container) is discovered + decoded by
/// a stock echo (another container) over cyclone RTPS on a shared domain —
/// entirely through `DockerRosEnv`, on a host without that edition installed.
#[test]
fn docker_edition_cyclone_pub_echo_smoke() {
    let ed = ros_env::test_edition();
    let domain = nros_tests::unique_ros_domain_id();
    let env = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: domain });

    if !env.available() {
        nros_tests::skip!(
            "{ed} image not built or docker absent — run `just ros_editions image {ed}`"
        );
    }

    // Long-lived publisher peer (RAII: dropped at end → container `docker kill`ed).
    let _pub = env
        .spawn(
            "int32-pub",
            "ros2 topic pub -r 5 /nros_smoke std_msgs/msg/Int32 '{data: 42}'",
        )
        .expect("spawn publisher peer");

    // One-shot subscriber in a second container on the same domain. The echo's
    // own `timeout` bounds discovery; a delivered sample prints `data: 42`.
    let out = env
        .run_text("timeout 25 ros2 topic echo --once /nros_smoke std_msgs/msg/Int32 2>&1")
        .expect("run echo");

    assert!(
        out.contains("data: 42"),
        "jazzy cyclone peer did not deliver through DockerRosEnv; echo output was:\n{out}"
    );
}
