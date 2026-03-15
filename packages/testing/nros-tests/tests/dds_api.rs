//! DDS RMW backend integration tests
//!
//! Tests the DDS examples (native-dds-talker, native-dds-listener) built
//! with dust-dds. Unlike zenoh tests, DDS uses brokerless peer-to-peer
//! discovery — no router or agent process is needed.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, build_dds_listener, build_dds_talker, dds_listener_binary, dds_talker_binary,
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
        output.contains("Published: data="),
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
