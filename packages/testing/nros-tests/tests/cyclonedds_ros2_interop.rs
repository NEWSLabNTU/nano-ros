//! CycloneDDS ↔ ROS 2 interoperability tests (Phase 183.5).
//!
//! nano-ros's CycloneDDS backend is meant to be wire-compatible with stock
//! `rmw_cyclonedds_cpp` (Phase 117's core goal). These tests put a nano-ros
//! Cyclone node and a stock ROS 2 `rmw_cyclonedds_cpp` node on a shared
//! `ROS_DOMAIN_ID` and check they exchange data over RTPS/SPDP — the test
//! analogue of `rmw_interop.rs` (zenoh ↔ ROS 2) and `xrce_ros2_interop.rs`.
//!
//! ## Status (2026-05-27): all functional tests pass — pub/sub, service, action.
//!
//! Stock-RMW Cyclone interop is done for pub/sub (both directions), services, and
//! actions (117.12 + 177.36) — verified against ROS 2 humble + `rmw_cyclonedds_cpp`:
//! - **`nano_to_ros2_pubsub`** — nano-ros Cyclone talker → stock `ros2 topic
//!   echo`: **PASSES** (un-`#[ignore]`d). Gated on `require_ros2_cyclonedds()`,
//!   so it skips cleanly without ROS 2.
//! - **`ros2_to_nano_pubsub`** — **PASSES** (un-`#[ignore]`d). The native-C
//!   listener *did* receive the samples all along; its stdout was block-
//!   buffered (glibc's 4 KiB default when piped), so the `Received:` lines
//!   never reached the harness inside the 10 s window. Fix:
//!   `setvbuf(stdout, NULL, _IOLBF, 0)` in `examples/native/c/listener` — not a
//!   wire-compat or discovery gap at all.
//! - **`service_nano_server_ros2_client`** — **PASSES** (un-`#[ignore]`d). The
//!   old 117.12.B.1 failure was a reply-writer match-gate bug:
//!   `service_send_reply` waited for `current_count > 0` on the reply writer,
//!   but Cyclone 0.10.5 under-reports `current_count` for a stock
//!   `rmw_cyclonedds_cpp` reply reader that is in fact discovered + waiting.
//!   Fix: write once the reader is *discovered* (`total_count > 0`) after a
//!   short grace, not only on `current_count > 0` (`src/service.cpp`); also
//!   verified by the C++ CTest `ros2_srv_e2e` (5/5). Needs the native Cyclone
//!   service fixture (`just native build-fixture-extras`, which builds it since
//!   Phase 177.31 fixed the example link gap); skips cleanly if absent.
//!
//! Run them with
//! `cargo nextest run -E 'binary(cyclonedds_ros2_interop)'` (none are
//! `#[ignore]`d now); they require the prerequisites below or skip cleanly.
//!
//! Prerequisites (else the tests skip cleanly):
//! - ROS 2 + `rmw_cyclonedds_cpp` (`require_ros2_cyclonedds`)
//! - the native Cyclone fixtures (`just cyclonedds setup` → `build/install` +
//!   the CMake/Corrosion `build-cyclonedds/` binaries, Phase 175)

use std::{path::Path, process::Command, time::Duration};

use nros_tests::{
    count_pattern,
    fixtures::{
        DEFAULT_ROS_DISTRO, ManagedProcess, Rmw, Ros2DdsProcess, build_native_c_example_rmw,
        require_ros2_cyclonedds,
    },
};

const TOPIC: &str = "/chatter";
const MSG_TYPE: &str = "std_msgs/msg/String";
const SRV: &str = "/add_two_ints";
const SRV_TYPE: &str = "example_interfaces/srv/AddTwoInts";
const ACTION: &str = "/fibonacci";
const ACTION_TYPE: &str = "example_interfaces/action/Fibonacci";

/// Resolve (building if needed) a native Cyclone C example binary, or skip when
/// the fixtures aren't set up (`just cyclonedds setup`).
fn nano_cyclone_c_binary(case: &str, binary: &str) -> std::path::PathBuf {
    build_native_c_example_rmw(case, binary, Rmw::Cyclonedds).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/c/{case} cyclonedds fixture not built (run `just cyclonedds setup`): {e:?}"
        )
    })
}

/// Spawn a nano-ros Cyclone binary on `domain_id`, wiring `LD_LIBRARY_PATH` to
/// the in-tree `libddsc` (mirrors `native_api.rs::spawn_cyclone_binary`).
fn spawn_nano_cyclone(binary: &Path, name: &str, domain_id: u8) -> ManagedProcess {
    let mut cmd = Command::new(binary);
    cmd.env("ROS_DOMAIN_ID", domain_id.to_string())
        .env("RUST_LOG", "info");
    let cyclone_lib = nros_tests::project_root().join("build/install/lib");
    let ld = match std::env::var_os("LD_LIBRARY_PATH") {
        Some(existing) if !existing.is_empty() => {
            let mut paths = vec![cyclone_lib];
            paths.extend(std::env::split_paths(&existing));
            std::env::join_paths(paths).expect("valid LD_LIBRARY_PATH")
        }
        _ => cyclone_lib.into_os_string(),
    };
    cmd.env("LD_LIBRARY_PATH", ld);
    ManagedProcess::spawn_command(cmd, name).unwrap_or_else(|_| panic!("Failed to start {name}"))
}

// =============================================================================
// Detection (always runs — reports the env, never fails)
// =============================================================================

#[test]
fn test_cyclonedds_ros2_detection() {
    let available = require_ros2_cyclonedds();
    eprintln!("ROS 2 + rmw_cyclonedds_cpp available: {available}");
}

// =============================================================================
// Pub/sub interop
// =============================================================================

/// nano-ros Cyclone talker → ROS 2 (`rmw_cyclonedds_cpp`) subscriber.
#[test]
fn test_cyclonedds_nano_to_ros2_pubsub() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    // PID-seeded so this test never shares a ROS_DOMAIN_ID with any other
    // concurrent interop test — across RMWs too (a fixed domain here could
    // collide with another RMW's PID-seeded `unique_ros_domain_id()` pick).
    let domain = nros_tests::unique_ros_domain_id();
    let talker_bin = nano_cyclone_c_binary("talker", "c_talker");

    // ROS 2 subscriber first, then the nano publisher.
    let mut ros2_sub = Ros2DdsProcess::topic_echo_cyclonedds_with_domain(
        TOPIC,
        MSG_TYPE,
        DEFAULT_ROS_DISTRO,
        domain,
    )
    .expect("start ros2 cyclone echo");
    std::thread::sleep(Duration::from_secs(2));
    let mut talker = spawn_nano_cyclone(&talker_bin, "nano-cyclone-talker", domain);

    let ros2_output = ros2_sub
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    talker.kill();

    eprintln!("ROS 2 cyclone subscriber output:\n{ros2_output}");
    let n = count_pattern(&ros2_output, "data:");
    assert!(
        n > 0,
        "ROS 2 cyclone subscriber received no samples from the nano talker, got:\n{ros2_output}"
    );
}

/// ROS 2 (`rmw_cyclonedds_cpp`) publisher → nano-ros Cyclone subscriber.
#[test]
fn test_cyclonedds_ros2_to_nano_pubsub() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    let domain = nros_tests::unique_ros_domain_id();
    let listener_bin = nano_cyclone_c_binary("listener", "c_listener");

    // nano subscriber first, then the ROS 2 publisher.
    let mut listener = spawn_nano_cyclone(&listener_bin, "nano-cyclone-listener", domain);
    std::thread::sleep(Duration::from_secs(3));
    let mut ros2_pub = Ros2DdsProcess::topic_pub_cyclonedds_with_domain(
        TOPIC,
        MSG_TYPE,
        "{data: 'Hello World: 42'}",
        5,
        DEFAULT_ROS_DISTRO,
        domain,
    )
    .expect("start ros2 cyclone pub");

    let listener_output = listener
        .wait_for_output_pattern(
            nros_tests::output::LISTENER_LOG_PREFIX,
            Duration::from_secs(10),
        )
        .unwrap_or_default();
    ros2_pub.kill();
    listener.kill();

    eprintln!("nano cyclone listener output:\n{listener_output}");
    assert!(
        listener_output.contains(nros_tests::output::LISTENER_LOG_PREFIX),
        "nano cyclone listener received no sample from the ROS 2 publisher, got:\n{listener_output}"
    );
}

// =============================================================================
// Service interop
// =============================================================================

/// nano-ros Cyclone service server ↔ ROS 2 (`rmw_cyclonedds_cpp`) client.
#[test]
fn test_cyclonedds_service_nano_server_ros2_client() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    let domain = nros_tests::unique_ros_domain_id();
    let server_bin = nano_cyclone_c_binary("service-server", "c_service_server");

    let mut server = spawn_nano_cyclone(&server_bin, "nano-cyclone-service-server", domain);
    // Services need queryable/endpoint discovery before the client call.
    std::thread::sleep(Duration::from_secs(4));
    let mut client = Ros2DdsProcess::service_call_cyclonedds_with_domain(
        SRV,
        SRV_TYPE,
        "{a: 5, b: 3}",
        DEFAULT_ROS_DISTRO,
        domain,
    )
    .expect("start ros2 cyclone service call");

    let client_output = client
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!("ROS 2 cyclone service-call output:\n{client_output}");
    assert!(
        client_output.contains("sum=8") || client_output.contains("response"),
        "ROS 2 cyclone client did not get the AddTwoInts reply (expected sum=8), got:\n{client_output}"
    );
}

// =============================================================================
// Action interop
// =============================================================================

/// nano-ros Cyclone action server ↔ ROS 2 (`rmw_cyclonedds_cpp`) action client.
///
/// `ros2 action send_goal --feedback /fibonacci example_interfaces/action/Fibonacci
/// "{order: 5}"` against the native Cyclone Fibonacci action server. Fibonacci(5)
/// → sequence `[0, 1, 1, 2, 3, 5]`.
///
/// **PASSES** (177.36). Two pieces unlocked it: the backend publishes
/// `ros_discovery_info` so stock ROS 2 sees the node + action server, and the
/// action server's feedback/status pubs now use the ROS 2 action QoS
/// (status=RELIABLE+TRANSIENT_LOCAL) so the client's status subscription matches
/// — `rcl_action_server_is_available` requires that, so best-effort never
/// completed `wait_for_server`.
#[test]
fn test_cyclonedds_action_nano_server_ros2_client() {
    if !require_ros2_cyclonedds() {
        nros_tests::skip!("ROS 2 + rmw_cyclonedds_cpp not available");
    }
    let domain = nros_tests::unique_ros_domain_id();
    let server_bin = nano_cyclone_c_binary("action-server", "c_action_server");

    let mut server = spawn_nano_cyclone(&server_bin, "nano-cyclone-action-server", domain);
    // Actions compose goal/result/cancel services + feedback/status pubs; give
    // the server time to register all of them before the client sends a goal.
    let _ = server.wait_for_output_pattern("Waiting for action goals", Duration::from_secs(8));
    std::thread::sleep(Duration::from_secs(2));

    let mut client = Ros2DdsProcess::action_send_goal_cyclonedds_with_domain(
        ACTION,
        ACTION_TYPE,
        "{order: 5}",
        DEFAULT_ROS_DISTRO,
        domain,
    )
    .expect("start ros2 cyclone action send_goal");

    let client_output = client
        .wait_for_output(Duration::from_secs(20))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!("ROS 2 cyclone action send_goal output:\n{client_output}");
    assert!(
        client_output.contains("sequence:") || client_output.contains("Result"),
        "ROS 2 cyclone action client did not get the Fibonacci result, got:\n{client_output}"
    );
}
