//! Issue 0976 — an OUTSIDE WITNESS for the Cyclone action wire format.
//!
//! `service.cpp` carries five per-action-type adapters that reshape CDR:
//! `strip_goal_id_len_at` removes a 4-byte length prefix before a goal UUID,
//! `strip_nested_cdr_at` removes a nested encapsulation header from inside a
//! message, and three more special-case `_SendGoal_*` / `_GetResult_*`. Two of
//! them DELETE BYTES — they correct a serialization that would otherwise be
//! wrong on the wire.
//!
//! Until this file, the only thing exercising them was
//! `test_native_cyclonedds_rust_action`, which is nano-ros server to nano-ros
//! client. Both ends share whatever convention the adapters implement, so that
//! test passes whether the bytes are ROS 2's or not — the single property the
//! adapters exist to provide is the one property it cannot observe. The message
//! and service paths already had witnesses (`ros2_pubsub_e2e`, `ros2_srv_e2e`);
//! actions had none.
//!
//! This is that witness: a stock `ros2 action send_goal`, over
//! `rmw_cyclonedds_cpp`, against the nano-ros action server. It is also what
//! unblocks issue 0970's service half — migrating `service.cpp` to a blob
//! sertype would change the action wire format, and nothing in the tree could
//! tell.

use std::process::Command;

use nros_tests::{
    fixtures,
    ros_env::{HostRosEnv, Middleware, RosEnv},
    ros2::{DEFAULT_ROS_DISTRO, require_ros2_cyclonedds},
};

/// A domain no concurrent copy of this test will pick.
///
/// Same scheme as the shell harnesses and `nros_tests::unique_ros_domain_id`:
/// a fixed domain is a shared bus, and two overlapping runs discover each
/// other's writers — which reads as a delivery bug rather than a collision
/// (issue 0580).
fn action_domain() -> u8 {
    nros_tests::unique_ros_domain_id()
}

/// Single-quote a path for the shell `RosEnv::run` builds.
///
/// The fixture path comes from the build tree, so it is ours rather than a
/// user's — but quoting it is one line and an unquoted path with a space
/// fails as "command not found", which reads as a missing fixture.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A real ROS 2 client drives a goal on the nano-ros action server.
///
/// Asserts the RESULT CONTENT, not merely that the call returned: the adapters
/// move bytes around inside the goal and result messages, so a wrong shape
/// shows up as a wrong sequence or a goal that is never accepted. Checking only
/// for a zero exit would pass on a server that accepted the goal and computed
/// nothing, which is the vacuous shape `check-no-vacuous-tests` exists for.
#[test]
fn a_stock_ros2_client_drives_the_nano_ros_action_server() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    let server_bin = fixtures::build_native_rust_example_rmw(
        "action-server",
        "action-server",
        fixtures::Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust cyclonedds action-server fixture: {e}");
    });

    let domain = action_domain();
    let mut server = Command::new(&server_bin)
        .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp")
        .env("ROS_DOMAIN_ID", domain.to_string())
        .spawn()
        .expect("start the nano-ros action server");

    // Discovery over DDS multicast is not instant, and `send_goal` on an
    // undiscovered action fails rather than waiting.
    std::thread::sleep(std::time::Duration::from_secs(6));

    // `RosEnv`, not a hand-rolled `source /opt/ros/...` (RFC-0058). A second
    // spelling drifts from the first invisibly: the last one dropped the peer's
    // ROS_DOMAIN_ID, and another hardcoded `humble` so every guarded test
    // skipped forever on a jazzy host. `check-ros-env-spelling` caught this
    // file doing exactly that.
    let env = HostRosEnv::new(
        DEFAULT_ROS_DISTRO,
        Middleware::Cyclonedds { domain_id: domain },
    );
    let out = env
        .run("timeout 60 ros2 action send_goal /fibonacci example_interfaces/action/Fibonacci '{order: 5}'")
        .expect("run ros2 action send_goal");

    let _ = server.kill();
    let _ = server.wait();

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        text.contains("Goal accepted"),
        "a stock ROS 2 client must get the goal ACCEPTED — if the goal-id \
         adapter reshapes the UUID wrongly the server never accepts.\n{text}"
    );
    assert!(
        text.contains("SUCCEEDED"),
        "the goal must reach SUCCEEDED, not merely be accepted.\n{text}"
    );
    // Fibonacci(5) — the result travels the `_GetResult_Response_` path, which
    // is the one carrying the hand-built struct workaround (phase 171.0.b).
    for n in ["0", "1", "2", "3", "5"] {
        assert!(
            text.contains(&format!("- {n}")),
            "the result sequence must contain {n}; a reshaped result decodes \
             to the wrong numbers rather than failing.\n{text}"
        );
    }
}

/// The nano-ros action CLIENT drives a stock ROS 2 action server.
///
/// The reverse of the cell above, and not redundant with it: the adapters sit
/// on BOTH sides of the service path. Sending a goal exercises
/// `strip_goal_id_len_at` and `strip_nested_cdr_at` on what nano-ros WRITES;
/// receiving feedback and a result exercises the take path against bytes a real
/// ROS 2 server produced. One direction passing says nothing about the other —
/// which is the whole reason a nano-ros-to-nano-ros test could not settle this.
///
/// The peer is `examples_rclcpp_minimal_action_server`, which serves
/// `/fibonacci` over `example_interfaces/action/Fibonacci` — the same action and
/// type the nano-ros client targets.
#[test]
fn the_nano_ros_action_client_drives_a_stock_ros2_server() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }

    let client_bin = fixtures::build_native_rust_example_rmw(
        "action-client",
        "action-client",
        fixtures::Rmw::Cyclonedds,
    )
    .unwrap_or_else(|e| {
        nros_tests::skip!("native rust cyclonedds action-client fixture: {e}");
    });

    let domain = action_domain();
    let env = HostRosEnv::new(
        DEFAULT_ROS_DISTRO,
        Middleware::Cyclonedds { domain_id: domain },
    );

    // `RosPeer` kills the whole process group on drop, so a failed assertion
    // below cannot leave a `ros2` server behind to collide with the next run —
    // the orphan-peer shape that makes a later test fail for an earlier test's
    // reason.
    let _server = env
        .spawn(
            "minimal_action_server",
            "ros2 run examples_rclcpp_minimal_action_server action_server_member_functions",
        )
        .expect("start the stock ROS 2 action server");

    std::thread::sleep(std::time::Duration::from_secs(6));

    // Through the ROS env with an explicit `timeout`, NOT a bare
    // `Command::output()`. The client blocks waiting for a server it has not
    // discovered, and `output()` has no deadline — the first version of this
    // test hung for ten minutes instead of failing in one. A test that cannot
    // fail in bounded time is not a test.
    let out = env
        .run(&format!(
            "timeout 60 {}",
            shell_escape(&client_bin.to_string_lossy())
        ))
        .expect("run the nano-ros action client");

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        text.contains("Goal accepted"),
        "a real ROS 2 server must ACCEPT the goal nano-ros wrote — this is the \
         side `strip_goal_id_len_at` and `strip_nested_cdr_at` reshape.\n{text}"
    );
    // Fibonacci(10). Asserted as the whole sequence: a reshaped result decodes
    // to wrong numbers rather than failing, so a substring like "Result" would
    // pass on garbage.
    assert!(
        text.contains("Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]"),
        "the result from a stock server must decode to Fibonacci(10).\n{text}"
    );
    // Feedback travels a different message than the result, and only the
    // feedback path exercises the server's own periodic publish.
    assert!(
        text.contains("Next number in sequence received"),
        "feedback from a stock server must reach the nano-ros client.\n{text}"
    );
}
