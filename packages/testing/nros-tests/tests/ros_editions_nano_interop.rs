//! phase-309 W5 residual — a REAL nano-ros node interoperates with a live ROS 2
//! edition peer through `domain_bridge`.
//!
//! The stock-publisher lane (`ros_editions_bridge.rs`) proves the harness + the
//! #0267 bridge topology against a non-host edition. THIS lane replaces the
//! stock `ros2 topic pub` with the `ros-edition-pose-pub` nano-ros CycloneDDS
//! node (built against an edition's generated `geometry_msgs` bindings by
//! `just ros_editions build-fixture <distro>`), so it exercises nano-ros's own
//! typed publish + depth-2 nested descriptor path (the #0267 fix) end-to-end
//! against that edition's live graph:
//!
//!   nano-ros pub (domain A) → jazzy domain_bridge (A→B) → jazzy echo (B).
//!
//! Skips (never silently passes) when the prebuilt fixture, docker, or the image
//! is absent. Not in `just ci`; run by `just ros_editions ci`.

use std::{path::PathBuf, process::Command};

use nros_tests::ros_env::{self, DockerRosEnv, Middleware, RosEnv};

fn fixture_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bins/ros-edition-pose-pub/target/debug/ros-edition-pose-pub")
}

#[test]
fn nano_ros_posestamped_survives_jazzy_domain_bridge() {
    let bin = fixture_bin();
    if !bin.is_file() {
        nros_tests::skip!(
            "nano-ros pose publisher fixture not built — run `just ros_editions build-fixture jazzy`"
        );
    }

    let base = nros_tests::unique_ros_domain_id();
    let (d_from, d_to) = if base >= 232 {
        (base, base - 1)
    } else {
        (base, base + 1)
    };
    let env_to = DockerRosEnv::new("jazzy", Middleware::Cyclonedds { domain_id: d_to });
    if !env_to.available() {
        nros_tests::skip!(
            "jazzy image not built or docker absent — run `just ros_editions image jazzy`"
        );
    }

    // Downstream echo on domain B.
    let mut echo = env_to
        .spawn(
            "echo",
            "timeout 50 ros2 topic echo --once /pose geometry_msgs/msg/PoseStamped 2>&1",
        )
        .expect("spawn echo");

    // Bridge A→B, then the REAL nano-ros publisher on domain A (cyclone RMW).
    let _bridge = env_to
        .spawn_domain_bridge("/pose", "geometry_msgs/msg/PoseStamped", d_from, d_to)
        .expect("spawn domain_bridge");

    let mut pub_cmd = Command::new(&bin);
    pub_cmd
        .env("ROS_DOMAIN_ID", d_from.to_string())
        .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp")
        .env("RUST_LOG", "warn");
    let _pub =
        ros_env::spawn_process(pub_cmd, "nano-ros-pose-pub").expect("spawn nano-ros publisher");

    let out = echo
        .wait_for_output(std::time::Duration::from_secs(55))
        .unwrap_or_default();

    for needle in ["sec: 7", "frame_id: map", "x: 1.5", "z: -3.5", "w: 1.0"] {
        assert!(
            out.contains(needle),
            "nano-ros PoseStamped value \"{needle}\" did not survive the jazzy domain_bridge \
             (d_from={d_from} d_to={d_to}); downstream output was:\n{out}"
        );
    }
}
