//! phase-309 W5 — testing axis: the #0267 `domain_bridge` topology as a harness
//! test against a NON-host ROS edition.
//!
//! A publisher (domain A) → `domain_bridge` (A→B) → echo (domain B), all in
//! jazzy containers over cyclone, driven entirely through `DockerRosEnv` on a
//! host with no jazzy install. Uses the exact #0267 shape
//! (`geometry_msgs/msg/PoseStamped` — nested Pose{Point,Quaternion}) so the lane
//! exercises depth-2 nested delivery across the bridge, the corruption issue
//! #0267 fixed. This is the reusable replacement for the manual
//! `scripts/ros/domain-bridge-repro.sh`.
//!
//! With a stock publisher it proves the harness can stand up the full topology
//! against an edition the host lacks. Swapping the publisher for a nano-ros
//! cyclone node (built against that edition's generated code) turns it into the
//! product interop test — see phase-309 W5 notes for the fixture hookup.
//!
//! Skips without docker/image. Not in `just ci`; run by `just ros_editions ci`.

use nros_tests::ros_env::{self, DockerRosEnv, Middleware, RosEnv};

#[test]
fn edition_domain_bridge_posestamped_survives() {
    let ed = ros_env::test_edition();
    let base = nros_tests::unique_ros_domain_id();
    let (d_from, d_to) = if base >= 232 {
        (base, base - 1)
    } else {
        (base, base + 1)
    };

    let env_from = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: d_from });
    let env_to = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: d_to });

    if !env_to.available() {
        nros_tests::skip!(
            "{ed} image not built or docker absent — run `just ros_editions image {ed}`"
        );
    }

    // Downstream echo on domain B (captures the first bridged sample).
    let mut echo = env_to
        .spawn(
            "echo",
            "timeout 50 ros2 topic echo --once /pose geometry_msgs/msg/PoseStamped 2>&1",
        )
        .expect("spawn echo");

    // The bridge A→B, then the publisher on domain A. RAII kills all on drop.
    let _bridge = env_to
        .spawn_domain_bridge("/pose", "geometry_msgs/msg/PoseStamped", d_from, d_to)
        .expect("spawn domain_bridge");
    let _pub = env_from
        .spawn(
            "pub",
            "ros2 topic pub -r 5 /pose geometry_msgs/msg/PoseStamped \
             '{header: {stamp: {sec: 7, nanosec: 9}, frame_id: map}, \
               pose: {position: {x: 1.5, y: 2.5, z: -3.5}, orientation: {w: 1.0}}}'",
        )
        .expect("spawn publisher");

    let out = echo
        .wait_for_output(std::time::Duration::from_secs(55))
        .unwrap_or_default();

    for needle in ["sec: 7", "frame_id: map", "x: 1.5", "z: -3.5", "w: 1.0"] {
        assert!(
            out.contains(needle),
            "value \"{needle}\" did not survive the jazzy domain_bridge \
             (d_from={d_from} d_to={d_to}); downstream output was:\n{out}"
        );
    }
}
