//! DDS RMW backend integration tests
//!
//! Tests the DDS examples (native-dds-talker, native-dds-listener) built
//! with dust-dds. Unlike zenoh tests, DDS uses brokerless peer-to-peer
//! discovery — no router or agent process is needed.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, build_dds_action_client, build_dds_action_server, build_dds_listener,
    build_dds_service_client, build_dds_service_server, build_dds_talker, dds_action_client_binary,
    dds_action_server_binary, dds_listener_binary, dds_service_client_binary,
    dds_service_server_binary, dds_talker_binary,
};
use rstest::rstest;
use std::path::PathBuf;
use std::time::Duration;

// =============================================================================
// Build Tests
// =============================================================================

#[test]
fn test_dds_talker_builds() {
    match build_dds_talker() {
        Ok(_) => {}
        Err(e) => panic!("DDS talker build failed: {e:?}"),
    }
}

#[test]
fn test_dds_listener_builds() {
    match build_dds_listener() {
        Ok(_) => {}
        Err(e) => panic!("DDS listener build failed: {e:?}"),
    }
}

// =============================================================================
// Startup Tests
// =============================================================================

#[rstest]
fn test_dds_talker_starts(dds_talker_binary: PathBuf) {
    let mut cmd = std::process::Command::new(&dds_talker_binary);
    cmd.env("RUST_LOG", "info");
    let mut proc =
        ManagedProcess::spawn_command(cmd, "dds-talker").expect("Failed to start dds-talker");

    std::thread::sleep(Duration::from_secs(3));

    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    assert!(
        output.contains("Publisher created"),
        "DDS talker failed to initialize.\nOutput:\n{output}"
    );
    assert!(
        output.contains("Published:"),
        "DDS talker did not publish any messages.\nOutput:\n{output}"
    );
}

#[rstest]
fn test_dds_listener_starts(dds_listener_binary: PathBuf) {
    let mut cmd = std::process::Command::new(&dds_listener_binary);
    cmd.env("RUST_LOG", "info");
    let mut proc =
        ManagedProcess::spawn_command(cmd, "dds-listener").expect("Failed to start dds-listener");

    std::thread::sleep(Duration::from_secs(3));

    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    assert!(
        output.contains("Subscriber created"),
        "DDS listener failed to initialize.\nOutput:\n{output}"
    );
}

// =============================================================================
// Communication Tests
// =============================================================================

#[rstest]
fn test_dds_talker_listener_communication(
    dds_talker_binary: PathBuf,
    dds_listener_binary: PathBuf,
) {
    // Start listener first
    let mut listener_cmd = std::process::Command::new(&dds_listener_binary);
    listener_cmd.env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "dds-listener")
        .expect("Failed to start dds-listener");

    // Give listener time to subscribe and SPDP to discover
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    let mut talker_cmd = std::process::Command::new(&dds_talker_binary);
    talker_cmd.env("RUST_LOG", "info");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "dds-talker")
        .expect("Failed to start dds-talker");

    // Wait for messages to flow (DDS discovery + data delivery)
    std::thread::sleep(Duration::from_secs(8));

    // Collect outputs
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("DDS talker output:\n{talker_output}");
    eprintln!("DDS listener output:\n{listener_output}");

    // Verify talker published
    let published = count_pattern(&talker_output, "Published");
    assert!(
        published >= 3,
        "Expected at least 3 published messages, got {published}.\nTalker output:\n{talker_output}"
    );

    // Verify listener received
    let received = count_pattern(&listener_output, "Received");
    assert!(
        received >= 3,
        "Expected at least 3 received messages, got {received}.\nListener output:\n{listener_output}"
    );
}

// =============================================================================
// Phase 95.F — DDS Service + Action Tests
// =============================================================================

#[test]
fn test_dds_service_server_builds() {
    build_dds_service_server().expect("DDS service server build failed");
}

#[test]
fn test_dds_service_client_builds() {
    build_dds_service_client().expect("DDS service client build failed");
}

#[test]
fn test_dds_action_server_builds() {
    build_dds_action_server().expect("DDS action server build failed");
}

#[test]
fn test_dds_action_client_builds() {
    build_dds_action_client().expect("DDS action client build failed");
}

/// E2E: DDS service server + client over RTPS (peer-to-peer, no broker).
///
/// **#[ignore]d**: dust-dds service request/reply SEDP discovery does
/// not match between two RTPS participants (server's request_DataReader
/// never sees the client's request_DataWriter, even on localhost).
/// Pubsub on the same configuration works fine
/// (`test_dds_talker_listener_communication`). Re-enable after a
/// Phase 71.x follow-up that tunes service-topic QoS (reliability +
/// history) and verifies the SEDP topic name format
/// (`rq<svc>Request` / `rr<svc>Reply`) matches what dust-dds
/// publishes via SEDP. The same #[ignore] applies to the
/// `test_zephyr_dds_rust_*_a9_e2e` cousins (Phase 95.B).
#[rstest]
#[ignore]
fn test_dds_service_server_client_e2e(
    dds_service_server_binary: PathBuf,
    dds_service_client_binary: PathBuf,
) {
    let mut server_cmd = std::process::Command::new(&dds_service_server_binary);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "dds-service-server")
        .expect("Failed to start dds-service-server");

    // Allow time for server's request_DataReader / reply_DataWriter
    // to come up before client probes the SEDP topology.
    std::thread::sleep(Duration::from_secs(3));

    let mut client_cmd = std::process::Command::new(&dds_service_client_binary);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "dds-service-client")
        .expect("Failed to start dds-service-client");

    // Client makes 4 calls × ~500 ms apart, plus 3 s pre-call discovery
    // sleep. 30 s is generous headroom for SPDP+SEDP under load.
    let client_output = client
        .wait_for_all_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("=== DDS service server output ===\n{server_output}");
    eprintln!("=== DDS service client output ===\n{client_output}");

    let responses = count_pattern(&client_output, "Response: ");
    assert!(
        responses >= 1,
        "Expected at least one service response, got {responses}.\nClient:\n{client_output}\nServer:\n{server_output}"
    );
}

/// E2E: DDS action server + client (Fibonacci) over RTPS.
///
/// **#[ignore]d** for the same reason as
/// `test_dds_service_server_client_e2e` — the action's 5-channel
/// service+pubsub composition fails at the same SEDP step.
#[rstest]
#[ignore]
fn test_dds_action_server_client_e2e(
    dds_action_server_binary: PathBuf,
    dds_action_client_binary: PathBuf,
) {
    let mut server_cmd = std::process::Command::new(&dds_action_server_binary);
    server_cmd.env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "dds-action-server")
        .expect("Failed to start dds-action-server");

    std::thread::sleep(Duration::from_secs(3));

    let mut client_cmd = std::process::Command::new(&dds_action_client_binary);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "dds-action-client")
        .expect("Failed to start dds-action-client");

    // Action client computes Fibonacci(10) → 11 feedback frames at
    // 500 ms each ≈ 5.5 s, plus get_result. 60 s budget covers
    // discovery + execution comfortably.
    let client_output = client
        .wait_for_all_output(Duration::from_secs(60))
        .unwrap_or_default();
    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("=== DDS action server output ===\n{server_output}");
    eprintln!("=== DDS action client output ===\n{client_output}");

    let feedback_count = count_pattern(&client_output, "Feedback");
    let completed = client_output.contains("completed") || client_output.contains("Result:");
    assert!(
        feedback_count >= 1 && completed,
        "DDS action E2E failed: feedback={feedback_count}, completed={completed}\nClient:\n{client_output}\nServer:\n{server_output}"
    );
}

// =============================================================================
// Phase 95.G + 95.H — Native C / C++ DDS example builds
// =============================================================================

use nros_tests::fixtures::{
    build_dds_c_action_client, build_dds_c_action_server, build_dds_c_listener,
    build_dds_c_service_client, build_dds_c_service_server, build_dds_c_talker,
    build_dds_cpp_action_client, build_dds_cpp_action_server, build_dds_cpp_listener,
    build_dds_cpp_service_client, build_dds_cpp_service_server, build_dds_cpp_talker,
};

#[test]
fn test_dds_c_talker_builds() { build_dds_c_talker().expect("c-dds-talker build"); }
#[test]
fn test_dds_c_listener_builds() { build_dds_c_listener().expect("c-dds-listener build"); }
#[test]
fn test_dds_c_service_server_builds() { build_dds_c_service_server().expect("c-dds-service-server build"); }
#[test]
fn test_dds_c_service_client_builds() { build_dds_c_service_client().expect("c-dds-service-client build"); }
#[test]
fn test_dds_c_action_server_builds() { build_dds_c_action_server().expect("c-dds-action-server build"); }
#[test]
fn test_dds_c_action_client_builds() { build_dds_c_action_client().expect("c-dds-action-client build"); }

#[test]
fn test_dds_cpp_talker_builds() { build_dds_cpp_talker().expect("cpp-dds-talker build"); }
#[test]
fn test_dds_cpp_listener_builds() { build_dds_cpp_listener().expect("cpp-dds-listener build"); }
#[test]
fn test_dds_cpp_service_server_builds() { build_dds_cpp_service_server().expect("cpp-dds-service-server build"); }
#[test]
fn test_dds_cpp_service_client_builds() { build_dds_cpp_service_client().expect("cpp-dds-service-client build"); }
#[test]
fn test_dds_cpp_action_server_builds() { build_dds_cpp_action_server().expect("cpp-dds-action-server build"); }
#[test]
fn test_dds_cpp_action_client_builds() { build_dds_cpp_action_client().expect("cpp-dds-action-client build"); }
