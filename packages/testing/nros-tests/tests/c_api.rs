//! C API integration tests
//!
//! Tests the C examples (c-talker, c-listener, c-service-server, c-service-client,
//! c-action-server, c-action-client) built with CMake.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_c_action_client, build_c_action_server, build_c_listener,
    build_c_service_client, build_c_service_server, build_c_talker, c_action_client_binary,
    c_action_server_binary, c_listener_binary, c_service_client_binary, c_service_server_binary,
    c_talker_binary, require_cmake, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Create a Command that wraps a C binary with `stdbuf -oL -eL` to force
/// line-buffered stdout/stderr. C's printf fully-buffers when piped.
fn stdbuf_command(binary: &Path) -> Command {
    let mut cmd = Command::new("stdbuf");
    cmd.args(["-oL", "-eL"]).arg(binary);
    cmd
}

// =============================================================================
// Build Tests
// =============================================================================

#[test]
fn test_c_talker_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_talker() {
        Ok(path) => {
            eprintln!("[PASS] C talker binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C talker: {}", e);
            panic!("C talker build failed: {}", e);
        }
    }
}

#[test]
fn test_c_listener_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_listener() {
        Ok(path) => {
            eprintln!("[PASS] C listener binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C listener: {}", e);
            panic!("C listener build failed: {}", e);
        }
    }
}

// =============================================================================
// Startup Tests
// =============================================================================

#[rstest]
fn test_c_talker_starts(zenohd_unique: ZenohRouter, c_talker_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_talker_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(cmd, "c-talker").expect("Failed to start c-talker");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C talker output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C talker failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_listener_starts(zenohd_unique: ZenohRouter, c_listener_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_listener_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut listener =
        ManagedProcess::spawn_command(cmd, "c-listener").expect("Failed to start c-listener");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C listener output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C listener failed to initialize.\nOutput:\n{}",
        output
    );
}

// =============================================================================
// Communication Tests
// =============================================================================

#[rstest]
fn test_c_talker_listener_communication(
    zenohd_unique: ZenohRouter,
    c_talker_binary: PathBuf,
    c_listener_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    // Start listener first
    let mut listener_cmd = stdbuf_command(&c_listener_binary);
    listener_cmd.env("ZENOH_LOCATOR", &locator);
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "c-listener")
        .expect("Failed to start c-listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(2));

    // Start talker
    let mut talker_cmd = stdbuf_command(&c_talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "c-talker").expect("Failed to start c-talker");

    // Wait for messages to flow
    std::thread::sleep(Duration::from_secs(6));

    // Kill talker first, but capture its output
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C talker output:\n{}", talker_output);

    // Collect listener output
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C listener output:\n{}", listener_output);

    // Verify initialization
    assert!(
        listener_output.contains("Support initialized"),
        "C listener failed to initialize.\nOutput:\n{}",
        listener_output
    );

    // Verify message reception (expect at least 3 messages)
    let received_count = count_pattern(&listener_output, "Received");
    eprintln!("C listener received {} messages", received_count);

    assert!(
        received_count >= 3,
        "Expected at least 3 messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}

// =============================================================================
// Service Build Tests
// =============================================================================

#[test]
fn test_c_service_server_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_service_server() {
        Ok(path) => {
            eprintln!("[PASS] C service server binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C service server: {}", e);
            panic!("C service server build failed: {}", e);
        }
    }
}

#[test]
fn test_c_service_client_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_service_client() {
        Ok(path) => {
            eprintln!("[PASS] C service client binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C service client: {}", e);
            panic!("C service client build failed: {}", e);
        }
    }
}

// =============================================================================
// Service Communication Tests
// =============================================================================

#[rstest]
fn test_c_service_server_starts(zenohd_unique: ZenohRouter, c_service_server_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_service_server_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(cmd, "c-service-server")
        .expect("Failed to start c-service-server");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C service server output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C service server failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_service_communication(
    zenohd_unique: ZenohRouter,
    c_service_server_binary: PathBuf,
    c_service_client_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    // Start service server first
    let mut server_cmd = stdbuf_command(&c_service_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "c-service-server")
        .expect("Failed to start c-service-server");

    // Wait for server to be ready
    std::thread::sleep(Duration::from_secs(3));

    // Start client
    let mut client_cmd = stdbuf_command(&c_service_client_binary);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "c-service-client")
        .expect("Failed to start c-service-client");

    // Wait for client to complete (it makes 4 blocking calls then exits)
    let client_output = client
        .wait_for_output_pattern("calls succeeded", Duration::from_secs(15))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    // Kill server
    server.kill();

    eprintln!("C service client output:\n{}", client_output);

    // Verify the client made successful calls
    let ok_count = count_pattern(&client_output, "[OK]");
    eprintln!("C service client: {} successful calls", ok_count);

    assert!(
        ok_count >= 3,
        "Expected at least 3 successful service calls, got {}.\nOutput:\n{}",
        ok_count,
        client_output
    );
}

// =============================================================================
// Action Build Tests
// =============================================================================

#[test]
fn test_c_action_server_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_action_server() {
        Ok(path) => {
            eprintln!("[PASS] C action server binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C action server: {}", e);
            panic!("C action server build failed: {}", e);
        }
    }
}

#[test]
fn test_c_action_client_builds() {
    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }
    match build_c_action_client() {
        Ok(path) => {
            eprintln!("[PASS] C action client binary built: {}", path.display());
            assert!(path.exists());
        }
        Err(e) => {
            eprintln!("[FAIL] Could not build C action client: {}", e);
            panic!("C action client build failed: {}", e);
        }
    }
}

// =============================================================================
// Action Communication Tests
// =============================================================================

#[rstest]
fn test_c_action_server_starts(zenohd_unique: ZenohRouter, c_action_server_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    let mut cmd = stdbuf_command(&c_action_server_binary);
    cmd.env("ZENOH_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(cmd, "c-action-server")
        .expect("Failed to start c-action-server");

    // Wait for initialization
    std::thread::sleep(Duration::from_secs(3));

    let output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("C action server output:\n{}", output);

    assert!(
        output.contains("Support initialized"),
        "C action server failed to initialize.\nOutput:\n{}",
        output
    );
}

#[rstest]
fn test_c_action_communication(
    zenohd_unique: ZenohRouter,
    c_action_server_binary: PathBuf,
    c_action_client_binary: PathBuf,
) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    // Start action server first
    let mut server_cmd = stdbuf_command(&c_action_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "c-action-server")
        .expect("Failed to start c-action-server");

    // Wait for server to be ready
    std::thread::sleep(Duration::from_secs(3));

    // Start client
    let mut client_cmd = stdbuf_command(&c_action_client_binary);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    let mut client = ManagedProcess::spawn_command(client_cmd, "c-action-client")
        .expect("Failed to start c-action-client");

    // Wait for client to complete
    let client_output = client
        .wait_for_output_pattern("Goodbye", Duration::from_secs(20))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    // Collect server output
    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    server.kill();

    eprintln!("=== C action server output ===\n{}", server_output);
    eprintln!("=== C action client output ===\n{}", client_output);

    // Verify the client sent a goal and got accepted
    assert!(
        client_output.contains("Goal accepted"),
        "C action client failed to send goal or get acceptance.\nOutput:\n{}",
        client_output
    );

    // Verify the server received and processed the goal
    assert!(
        server_output.contains("ACCEPTED") || server_output.contains("Executing goal"),
        "C action server did not process the goal.\nServer output:\n{}",
        server_output
    );

    eprintln!("[PASS] C action server/client communication works");
}

// =============================================================================
// Cross-language Interop Tests (C ↔ Rust)
// =============================================================================

#[rstest]
fn test_c_rust_pubsub_interop(zenohd_unique: ZenohRouter, c_talker_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    // Build Rust listener
    let rust_listener = match nros_tests::fixtures::build_native_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build Rust listener: {}", e);
            return;
        }
    };

    // Start Rust listener first
    let mut listener_cmd = Command::new(&rust_listener);
    listener_cmd.env("ZENOH_LOCATOR", &locator);
    listener_cmd.env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "rust-listener")
        .expect("Failed to start Rust listener");

    // Give listener time to subscribe
    std::thread::sleep(Duration::from_secs(2));

    // Start C talker
    let mut talker_cmd = stdbuf_command(&c_talker_binary);
    talker_cmd.env("ZENOH_LOCATOR", &locator);
    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "c-talker").expect("Failed to start C talker");

    // Wait for messages to flow
    std::thread::sleep(Duration::from_secs(6));

    // Kill talker
    talker.kill();

    // Collect listener output
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("Rust listener output (C talker):\n{}", listener_output);

    // Verify Rust listener received messages from C talker
    let received_count = count_pattern(&listener_output, "Received");
    eprintln!(
        "Rust listener received {} messages from C talker",
        received_count
    );

    assert!(
        received_count >= 2,
        "Expected at least 2 cross-language messages, got {}.\nOutput:\n{}",
        received_count,
        listener_output
    );
}

#[rstest]
fn test_c_rust_service_interop(zenohd_unique: ZenohRouter, c_service_server_binary: PathBuf) {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    if !require_cmake() {
        nros_tests::skip!("cmake not found");
    }

    let locator = zenohd_unique.locator();

    // Build Rust service client
    let rust_client = match nros_tests::fixtures::build_native_service_client() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build Rust service client: {}", e);
            return;
        }
    };

    // Start C service server
    let mut server_cmd = stdbuf_command(&c_service_server_binary);
    server_cmd.env("ZENOH_LOCATOR", &locator);
    let mut server = ManagedProcess::spawn_command(server_cmd, "c-service-server")
        .expect("Failed to start C service server");

    // Wait for server to register service queryable with zenohd
    std::thread::sleep(Duration::from_secs(5));

    // Start Rust client
    let mut client_cmd = Command::new(&rust_client);
    client_cmd.env("ZENOH_LOCATOR", &locator);
    client_cmd.env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "rust-service-client")
        .expect("Failed to start Rust service client");

    // Wait for client to complete (4 calls × 5s timeout + 3 × 500ms sleep ≈ 22s)
    let client_output = client
        .wait_for_output_pattern("completed successfully", Duration::from_secs(30))
        .or_else(|_| client.wait_for_all_output(Duration::from_secs(2)))
        .unwrap_or_default();

    server.kill();

    eprintln!("Rust client output (C server):\n{}", client_output);

    let response_count = count_pattern(&client_output, "Response:");
    eprintln!(
        "Rust client received {} responses from C server",
        response_count
    );

    assert!(
        response_count >= 2,
        "Expected at least 2 cross-language service responses, got {}.\nOutput:\n{}",
        response_count,
        client_output
    );
}
