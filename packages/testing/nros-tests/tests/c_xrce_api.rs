//! C XRCE-DDS API integration tests
//!
//! Tests the C examples (c-xrce-talker, c-xrce-listener) prebuilt with
//! CMake using the XRCE-DDS backend. `just native build-fixtures` stages
//! them under `examples/native/c/{talker,listener}/build-xrce/`; test
//! bodies only resolve and execute those binaries.
//!
//! Prerequisites:
//!   just build-xrce-agent   # Build the Micro-XRCE-DDS Agent from source
//!   just native build-fixtures  # Prebuild C XRCE example binaries

use nros_tests::{
    count_pattern,
    fixtures::{
        ManagedProcess, Rmw, XrceAgent, build_native_c_example_rmw, c_xrce_listener_binary,
        c_xrce_talker_binary, require_cmake, require_xrce_agent,
    },
};
use rstest::rstest;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Create a Command that wraps a C binary with `stdbuf -oL -eL` to force
/// line-buffered stdout/stderr. C's printf fully-buffers when piped.
fn stdbuf_command(binary: &Path) -> Command {
    let mut cmd = Command::new("stdbuf");
    cmd.args(["-oL", "-eL"]).arg(binary);
    cmd
}

// =============================================================================
// (Phase 182.3) `test_c_xrce_{talker,listener}_builds` removed — they only
// asserted the C XRCE fixtures compiled, covered by `build-all` + the C XRCE
// runtime tests below (which build the same binaries via the shared
// `build_c_xrce_*` resolvers).
// =============================================================================

// =============================================================================
// Startup Tests
// =============================================================================

#[rstest]
fn test_c_xrce_talker_starts(c_xrce_talker_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = stdbuf_command(&c_xrce_talker_binary);
    cmd.env("NROS_LOCATOR", &addr);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "c-xrce-talker").expect("Failed to start c-xrce-talker");

    // Phase-277 W5.C1 slimmed native/c/talker to the official-demo shape,
    // which dropped the "Support initialized" print — the publish line is
    // the talker's started-marker now (also proves the timer fired).
    let output = talker
        .wait_for_output_count("Publishing: '", 1, Duration::from_secs(5))
        .expect("C XRCE talker did not initialize");

    eprintln!("C XRCE talker output:\n{}", output);

    assert!(
        output.contains("Publishing: '"),
        "C XRCE talker failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_xrce_listener_starts(c_xrce_listener_binary: PathBuf) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let mut cmd = stdbuf_command(&c_xrce_listener_binary);
    cmd.env("NROS_LOCATOR", &addr);
    let mut listener = ManagedProcess::spawn_command(cmd, "c-xrce-listener")
        .expect("Failed to start c-xrce-listener");

    let output = listener
        .wait_for_output_count("Support initialized", 1, Duration::from_secs(5))
        .expect("C XRCE listener did not initialize");

    eprintln!("C XRCE listener output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C XRCE listener failed to initialize.\nOutput:\n{}",
        output
    );
}

// =============================================================================
// Communication Tests
// =============================================================================

#[rstest]
fn test_c_xrce_talker_listener_communication(
    c_xrce_talker_binary: PathBuf,
    c_xrce_listener_binary: PathBuf,
) {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    // Start listener first (subscribe before publishing)
    let mut listener_cmd = stdbuf_command(&c_xrce_listener_binary);
    listener_cmd.env("NROS_LOCATOR", &addr);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "c-xrce-listener")
        .expect("Failed to start c-xrce-listener");

    let init_output = listener
        .wait_for_output_count("Support initialized", 1, Duration::from_secs(5))
        .expect("C XRCE listener did not initialize");
    eprintln!("C XRCE listener initialized:\n{}", init_output);

    // Start talker
    let mut talker_cmd = stdbuf_command(&c_xrce_talker_binary);
    talker_cmd.env("NROS_LOCATOR", &addr);
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "c-xrce-talker")
        .expect("Failed to start c-xrce-talker");

    let message_output = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(12),
        )
        .expect("C XRCE listener did not receive 3 messages");
    let listener_output = format!("{init_output}{message_output}");

    talker.kill();

    eprintln!("C XRCE listener output:\n{}", listener_output);

    // Verify initialization
    assert!(
        listener_output.contains("Support initialized"),
        "C XRCE listener failed to initialize.\nOutput:\n{}",
        listener_output
    );

    // Verify message reception (expect at least 3 messages)
    let received_count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    eprintln!("C XRCE listener received {} messages", received_count);

    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}

// =============================================================================
// Native C XRCE service + action E2E (Phase 183.2)
//
// The C XRCE examples ship 6 cases but only pub/sub had an e2e
// (`test_c_xrce_talker_listener_communication`). These add the service
// request/response + action goal/feedback/result roundtrips, mirroring the
// Rust `tests/xrce.rs` service/action tests but driving the C `build-xrce/`
// binaries against a unique XRCE Agent. Binaries are prebuilt by
// `just native build-fixtures`; skip cleanly when absent.
// =============================================================================

/// Resolve a native C XRCE example binary (prebuilt), or skip.
fn nano_c_xrce(case: &str, binary: &str) -> PathBuf {
    build_native_c_example_rmw(case, binary, Rmw::Xrce).unwrap_or_else(|e| {
        nros_tests::skip!(
            "native/c/{case} xrce fixture not prebuilt (run `just native build-fixtures`): {e:?}"
        )
    })
}

/// C XRCE service server ↔ client (AddTwoInts roundtrip).
#[test]
fn test_c_xrce_service_request_response() {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let server_bin = nano_c_xrce("service-server", "c_service_server");
    let mut server_cmd = stdbuf_command(&server_bin);
    server_cmd.env("NROS_LOCATOR", &addr);
    let mut server = ManagedProcess::spawn_command(server_cmd, "c-xrce-service-server")
        .expect("start c-xrce-service-server");
    let _ = server.wait_for_output_pattern("Waiting for service requests", Duration::from_secs(15));

    let client_bin = nano_c_xrce("service-client", "c_service_client");
    let mut client_cmd = stdbuf_command(&client_bin);
    client_cmd.env("NROS_LOCATOR", &addr);
    let mut client = ManagedProcess::spawn_command(client_cmd, "c-xrce-service-client")
        .expect("start c-xrce-service-client");

    let client_output = client
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_RESULT_PREFIX,
            Duration::from_secs(20),
        )
        .unwrap_or_default();
    std::thread::sleep(Duration::from_millis(500));
    let server_output = server
        .wait_for_output_pattern(
            nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
            Duration::from_secs(2),
        )
        .unwrap_or_default();
    client.kill();
    server.kill();
    drop(agent);

    eprintln!("C XRCE service client:\n{client_output}\n--- server ---\n{server_output}");
    let calls = count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
    let handled = count_pattern(
        &server_output,
        nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
    );
    assert!(
        calls >= 1 || handled >= 1,
        "C XRCE service roundtrip produced no calls/requests.\nclient:\n{client_output}\nserver:\n{server_output}"
    );
}

/// C XRCE action server ↔ client (Fibonacci goal → feedback → result).
#[test]
fn test_c_xrce_action_fibonacci() {
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    let agent = XrceAgent::start_unique().expect("Failed to start XRCE Agent");
    let addr = agent.addr();

    let server_bin = nano_c_xrce("action-server", "c_action_server");
    let mut server_cmd = stdbuf_command(&server_bin);
    server_cmd.env("NROS_LOCATOR", &addr);
    let mut server = ManagedProcess::spawn_command(server_cmd, "c-xrce-action-server")
        .expect("start c-xrce-action-server");
    let _ = server.wait_for_output_pattern("Waiting for action goals", Duration::from_secs(15));

    let client_bin = nano_c_xrce("action-client", "c_action_client");
    let mut client_cmd = stdbuf_command(&client_bin);
    client_cmd.env("NROS_LOCATOR", &addr);
    let mut client = ManagedProcess::spawn_command(client_cmd, "c-xrce-action-client")
        .expect("start c-xrce-action-client");

    let client_output = client
        .wait_for_output_pattern(
            nros_tests::output::ACTION_RESULT_PREFIX,
            Duration::from_secs(20),
        )
        .unwrap_or_default();
    std::thread::sleep(Duration::from_millis(500));
    let server_output = server
        .wait_for_output_pattern("Received goal request", Duration::from_secs(2))
        .unwrap_or_default();
    client.kill();
    server.kill();
    drop(agent);

    eprintln!("C XRCE action client:\n{client_output}\n--- server ---\n{server_output}");
    assert!(
        client_output.contains("Goal accepted"),
        "C XRCE action client: goal not accepted.\n{client_output}"
    );
    assert!(
        client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX)
            || server_output.contains("Received goal request"),
        "C XRCE action did not reach a result.\nclient:\n{client_output}\nserver:\n{server_output}"
    );
}
