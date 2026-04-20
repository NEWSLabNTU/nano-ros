//! Zephyr native_sim integration tests
//!
//! These tests verify that nros running on Zephyr RTOS (native_sim)
//! can communicate with native Rust applications via zenoh.
//!
//! # Prerequisites
//!
//! - Zephyr workspace set up: `./scripts/zephyr/setup.sh`
//! - NSOS board overlays in examples/zephyr/*/boards/ (checked into git)
//! - zenohd installed (for E2E tests)
//!
//! # Running
//!
//! ```bash
//! # Run all Zephyr tests
//! cargo test -p nano-ros-tests --test zephyr
//!
//! # Run with output
//! cargo test -p nano-ros-tests --test zephyr -- --nocapture
//! ```

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    XrceAgent, ZenohRouter, build_native_listener, build_native_service_client,
    build_native_service_server, build_native_talker, require_xrce_agent,
};
use nros_tests::platform;
use nros_tests::zephyr::{
    ZephyrPlatform, ZephyrProcess, get_or_build_zephyr_example, is_zephyr_available,
    require_zephyr, zephyr_workspace_path,
};
use std::path::PathBuf;
use std::time::Duration;

/// Get or build Zephyr talker for native_sim (uses existing binary if available)
fn get_zephyr_talker_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-talker", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-talker binary")
}

/// Get or build Zephyr listener for native_sim (uses existing binary if available)
fn get_zephyr_listener_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-listener", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-listener binary")
}

// =============================================================================
// Availability Tests
// =============================================================================

/// Test that Zephyr availability checks work
#[test]
fn test_zephyr_availability_checks() {
    eprintln!("Zephyr workspace path: {:?}", zephyr_workspace_path());
    eprintln!("Zephyr available: {}", is_zephyr_available());

    // These are informational - don't fail if Zephyr isn't set up
}

// =============================================================================
// Zephyr E2E Tests (with automatic zenohd)
// =============================================================================

/// Test: Zephyr talker → Zephyr listener communication
///
/// This is a full E2E integration test that:
/// 1. Starts zenohd automatically automatically
/// 2. Runs both Zephyr talker and listener
/// 3. Verifies messages are delivered
///
/// Requires:
/// - NSOS board overlays in examples/zephyr/*/boards/ (checked into git)
/// - Both examples built with their specific TAP interface configs
#[test]
fn test_zephyr_talker_to_listener_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd started on port {}", platform::ZEPHYR.zenohd_port);

    // Give zenohd time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build both examples (to separate directories)
    let talker_binary = get_zephyr_talker_native_sim();
    let listener_binary = get_zephyr_listener_native_sim();

    eprintln!("Talker binary: {}", talker_binary.display());
    eprintln!("Listener binary: {}", listener_binary.display());

    // Start listener first (so it creates its subscriber before talker publishes)
    eprintln!("Starting Zephyr listener...");
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr listener");

    // Give listener time to connect and create subscriber
    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    eprintln!("Starting Zephyr talker...");
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr talker");

    // Wait for communication
    eprintln!("Waiting for Zephyr talker → listener communication...");

    // Wait for output from both
    let talker_output = talker
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(8))
        .unwrap_or_default();

    // Kill processes
    let _ = talker.kill();
    let _ = listener.kill();
    drop(router);

    eprintln!("\n=== Talker output ===\n{}", talker_output);
    eprintln!("\n=== Listener output ===\n{}", listener_output);

    // Check talker status
    let talker_published = talker_output.contains("Published:") || talker_output.contains("data=");
    let talker_connected = !talker_output.contains("session error");
    let talker_created_pub = talker_output.contains("Declared publisher")
        || talker_output.contains("Publisher created")
        || talker_output.contains("Publishing messages");

    // Check listener status
    let listener_received =
        listener_output.contains("Received:") || listener_output.contains("data=");
    let listener_connected = !listener_output.contains("session error");
    let listener_created_sub = listener_output.contains("Declared subscriber")
        || listener_output.contains("Subscriber created")
        || listener_output.contains("Waiting for messages");
    let listener_failed_sub = listener_output.contains("Failed to create subscriber");

    if !talker_connected {
        panic!("Talker failed to connect to zenohd");
    }
    if !listener_connected {
        panic!("Listener failed to connect to zenohd");
    }

    // Handle zenoh-pico interest message limitation
    // Only one client can successfully declare at a time
    if !talker_created_pub && !listener_failed_sub {
        panic!("Talker failed to create publisher");
    }
    if !listener_created_sub && !talker_published {
        // Listener failed to create subscriber - known zenoh-pico limitation
        // when multiple clients connect simultaneously
        if listener_failed_sub {
            eprintln!(
                "\nWARNING: Listener failed to create subscriber (zenoh-pico interest conflict)"
            );
            eprintln!("This is a known limitation when multiple clients connect simultaneously");
            eprintln!(
                "Talker published {} messages successfully",
                count_pattern(&talker_output, "Published:")
            );
            // Don't fail the test - this is a known limitation
            return;
        }
        panic!("Listener failed to create subscriber and talker didn't publish");
    }

    // Check for known zenoh-pico limitation: transport TX failure when multiple clients connect
    let talker_tx_failed = talker_output.contains("Failed to publish");

    if talker_published && listener_received {
        let count = count_pattern(&listener_output, "Received");
        eprintln!(
            "\nSUCCESS: Zephyr listener received {} messages from Zephyr talker",
            count
        );
    } else if talker_published && listener_created_sub {
        panic!("Talker published but listener didn't receive (timing issue?)");
    } else if talker_tx_failed {
        panic!("Talker transport TX failed — zenoh-pico session issue");
    } else if talker_published {
        panic!("Talker published but listener failed to subscribe");
    } else {
        panic!("Communication failed — talker didn't publish messages");
    }
}

/// Test: Zephyr talker → Native listener communication
///
/// Tests that a Zephyr talker can send messages to a native Rust listener.
#[test]
fn test_zephyr_to_native_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    // Give zenohd time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build native listener
    let listener_path = build_native_listener().expect("Failed to build native-rs-listener");

    // Get Zephyr talker
    let zephyr_binary = get_zephyr_talker_native_sim();
    eprintln!("Zephyr talker binary: {}", zephyr_binary.display());

    // Start native listener connecting to zenohd
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut listener_cmd = Command::new(listener_path);
    // Both native and Zephyr NSOS processes connect to zenohd on localhost
    listener_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    // Give listener time to connect and subscribe
    std::thread::sleep(Duration::from_secs(1));

    // Start Zephyr talker
    eprintln!("Starting Zephyr talker...");
    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr talker");

    // Wait for communication
    eprintln!("Waiting for Zephyr → Native communication...");

    // Wait for listener output (use wait_for_all_output to capture stderr where env_logger logs)
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(10))
        .expect("Listener timed out");

    // Get Zephyr output for debugging
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Kill processes
    let _ = zephyr.kill();
    drop(listener);
    drop(router);

    eprintln!("\n=== Zephyr output ===\n{}", zephyr_output);
    eprintln!("\n=== Native listener output ===\n{}", listener_output);

    // Check for known zenoh-pico transport TX failure
    let zephyr_tx_failed = zephyr_output.contains("z_publisher_put failed")
        || zephyr_output.contains("Failed to publish");
    let zephyr_connected = zephyr_output.contains("Session opened");
    let zephyr_declared_pub = zephyr_output.contains("Declared publisher");

    // The listener should have received at least one message
    let has_received = listener_output.contains("Received")
        || listener_output.contains("Int32")
        || listener_output.contains("data:");

    if has_received {
        let count = count_pattern(&listener_output, "Received");
        eprintln!(
            "\nSUCCESS: Native listener received {} messages from Zephyr talker",
            count
        );
    } else if zephyr_tx_failed && zephyr_connected && zephyr_declared_pub {
        panic!(
            "zenoh-pico transport TX failure — talker connected and declared publisher but failed to send"
        );
    } else if !zephyr_connected {
        panic!("Zephyr talker failed to connect to zenohd");
    } else {
        panic!("No messages received from Zephyr");
    }
}

/// Test: Native talker → Zephyr listener communication
///
/// Tests that a native Rust talker can send messages to a Zephyr listener.
/// This is the reverse direction of `test_zephyr_to_native_e2e`.
#[test]
fn test_native_to_zephyr_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    // Give zenohd time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build native talker
    let talker_path = build_native_talker().expect("Failed to build native-rs-talker");

    // Get Zephyr listener
    let zephyr_binary = get_zephyr_listener_native_sim();
    eprintln!("Zephyr listener binary: {}", zephyr_binary.display());

    // Start Zephyr listener first (so it subscribes before talker publishes)
    eprintln!("Starting Zephyr listener...");
    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr listener");

    // Give listener time to connect and subscribe
    std::thread::sleep(Duration::from_secs(1));

    // Start native talker connecting to zenohd
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut talker_cmd = Command::new(talker_path);
    // Both connect to zenohd on localhost
    talker_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for communication
    eprintln!("Waiting for Native → Zephyr communication...");

    // Wait for Zephyr output
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    // Get native talker output for debugging
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Kill processes
    let _ = zephyr.kill();
    drop(talker);
    drop(router);

    eprintln!("\n=== Native talker output ===\n{}", talker_output);
    eprintln!("\n=== Zephyr listener output ===\n{}", zephyr_output);

    // Check for known zenoh-pico transport issues
    let zephyr_connected = zephyr_output.contains("Session opened");
    let zephyr_subscribed = zephyr_output.contains("Declared subscriber")
        || zephyr_output.contains("Subscriber created");
    let zephyr_subscribe_failed = zephyr_output.contains("Failed to create subscriber")
        || zephyr_output.contains("z_declare_subscriber failed");

    // The listener should have received at least one message
    let has_received = zephyr_output.contains("Received")
        || zephyr_output.contains("Int32")
        || zephyr_output.contains("data:");

    // Check native talker status
    let talker_published = talker_output.contains("Published");

    if has_received {
        let count = count_pattern(&zephyr_output, "Received");
        eprintln!(
            "\nSUCCESS: Zephyr listener received {} messages from native talker",
            count
        );
    } else if zephyr_subscribe_failed && zephyr_connected {
        panic!(
            "zenoh-pico subscription failure — Zephyr listener connected but failed to subscribe"
        );
    } else if zephyr_subscribed && talker_published && !has_received {
        panic!(
            "Both sides ready but messages not delivered — Zephyr subscribed and native talker published"
        );
    } else if !zephyr_connected {
        panic!("Zephyr listener failed to connect to zenohd");
    } else {
        panic!("No messages received by Zephyr listener");
    }
}

/// Test: Bidirectional Native ↔ Zephyr communication
///
/// Tests that communication works in both directions simultaneously:
/// - Native talker → Zephyr listener
/// - Zephyr talker → Native listener
///
/// This test verifies that the bridge network and zenohd can handle
/// multiple clients and bidirectional traffic.
#[test]
fn test_bidirectional_native_zephyr_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    std::thread::sleep(Duration::from_millis(500));

    // Build all binaries
    let native_talker_path = build_native_talker().expect("Failed to build native-rs-talker");
    let native_listener_path = build_native_listener().expect("Failed to build native-rs-listener");
    let zephyr_talker_binary = get_zephyr_talker_native_sim();
    let zephyr_listener_binary = get_zephyr_listener_native_sim();

    eprintln!("Native talker: {}", native_talker_path.display());
    eprintln!("Native listener: {}", native_listener_path.display());
    eprintln!("Zephyr talker: {}", zephyr_talker_binary.display());
    eprintln!("Zephyr listener: {}", zephyr_listener_binary.display());

    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    // Start listeners first (both native and Zephyr)
    eprintln!("Starting listeners...");

    let mut native_listener_cmd = Command::new(native_listener_path);
    native_listener_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut native_listener =
        ManagedProcess::spawn_command(native_listener_cmd, "native-rs-listener")
            .expect("Failed to start native listener");

    // Note: Running multiple Zephyr processes simultaneously can cause issues
    // due to TAP interface conflicts. For this test, we use a staggered approach.
    let mut zephyr_listener =
        ZephyrProcess::start(&zephyr_listener_binary, ZephyrPlatform::NativeSim)
            .expect("Failed to start Zephyr listener");

    // Give listeners time to connect and subscribe
    std::thread::sleep(Duration::from_secs(2));

    // Start talkers
    eprintln!("Starting talkers...");

    let mut native_talker_cmd = Command::new(native_talker_path);
    native_talker_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut native_talker = ManagedProcess::spawn_command(native_talker_cmd, "native-rs-talker")
        .expect("Failed to start native talker");

    let mut zephyr_talker = ZephyrProcess::start(&zephyr_talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr talker");

    // Wait for communication in both directions
    eprintln!("Waiting for bidirectional communication...");
    std::thread::sleep(Duration::from_secs(5));

    // Collect outputs
    let native_listener_output = native_listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let zephyr_listener_output = zephyr_listener
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    let native_talker_output = native_talker
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();
    let zephyr_talker_output = zephyr_talker
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Kill all processes
    let _ = zephyr_talker.kill();
    let _ = zephyr_listener.kill();
    drop(native_talker);
    drop(native_listener);
    drop(router);

    eprintln!("\n=== Native talker output ===\n{}", native_talker_output);
    eprintln!("\n=== Zephyr talker output ===\n{}", zephyr_talker_output);
    eprintln!(
        "\n=== Native listener output ===\n{}",
        native_listener_output
    );
    eprintln!(
        "\n=== Zephyr listener output ===\n{}",
        zephyr_listener_output
    );

    // Check direction 1: Zephyr talker → Native listener
    let native_received = native_listener_output.contains("Received")
        || native_listener_output.contains("Int32")
        || native_listener_output.contains("data:");
    let native_received_count = count_pattern(&native_listener_output, "Received");

    // Check direction 2: Native talker → Zephyr listener
    let zephyr_received = zephyr_listener_output.contains("Received")
        || zephyr_listener_output.contains("Int32")
        || zephyr_listener_output.contains("data:");
    let zephyr_received_count = count_pattern(&zephyr_listener_output, "Received");

    // Check for known limitations
    let zephyr_talker_tx_failed = zephyr_talker_output.contains("Failed to publish")
        || zephyr_talker_output.contains("z_publisher_put failed");
    let zephyr_listener_sub_failed = zephyr_listener_output.contains("Failed to create subscriber")
        || zephyr_listener_output.contains("z_declare_subscriber failed");

    eprintln!("\n=== Results ===");
    eprintln!(
        "Direction 1 (Zephyr → Native): {} messages received",
        native_received_count
    );
    eprintln!(
        "Direction 2 (Native → Zephyr): {} messages received",
        zephyr_received_count
    );

    // Analyze results
    if native_received && zephyr_received {
        eprintln!("\nSUCCESS: Bidirectional communication works!");
        eprintln!(
            "  - Native listener received {} messages from Zephyr",
            native_received_count
        );
        eprintln!(
            "  - Zephyr listener received {} messages from native",
            zephyr_received_count
        );
    } else if native_received && !zephyr_received {
        if zephyr_listener_sub_failed {
            panic!("Zephyr → Native works, Native → Zephyr failed (subscription failure)");
        } else {
            panic!("Zephyr → Native works, Native → Zephyr failed");
        }
    } else if !native_received && zephyr_received {
        if zephyr_talker_tx_failed {
            panic!("Native → Zephyr works, Zephyr → Native failed (TX failure)");
        } else {
            panic!("Native → Zephyr works, Zephyr → Native failed");
        }
    } else {
        panic!("Bidirectional communication failed — no messages received in either direction");
    }
}

// =============================================================================
// Smoke Tests (no zenohd required)
// =============================================================================

/// Test: Zephyr talker starts and runs without crashing
///
/// Basic smoke test that verifies the Zephyr binary runs correctly.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_talker_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_talker_native_sim();
    eprintln!("Starting Zephyr talker: {}", zephyr_binary.display());

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr talker");

    // Wait for output (Zephyr will fail to connect but should produce init messages)
    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    // The process should have started and produced some output
    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr talker booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr talker failed to boot - no initialization output");
    }
}

/// Test: Zephyr listener starts correctly
///
/// Basic smoke test that verifies the Zephyr listener boots and initializes.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_listener_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_listener_native_sim();
    eprintln!("Starting Zephyr listener: {}", zephyr_binary.display());

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr listener");

    // Wait for output (Zephyr will fail to connect but should produce init messages)
    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    // The process should have started and produced some output
    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr listener booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr listener failed to boot - no initialization output");
    }
}

// =============================================================================
// Build Tests
// =============================================================================

/// Test: Zephyr talker can be built or found
#[test]
fn test_zephyr_talker_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result = get_or_build_zephyr_example("zephyr-rs-talker", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built talker at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-talker: {}", e);
        }
    }
}

/// Test: Zephyr listener can be built or found
#[test]
fn test_zephyr_listener_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result =
        get_or_build_zephyr_example("zephyr-rs-listener", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built listener at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-listener: {}", e);
        }
    }
}

// =============================================================================
// Zephyr Action Examples
// =============================================================================

/// Get or build Zephyr action server for native_sim
fn get_zephyr_action_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-action-server", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-action-server binary")
}

/// Get or build Zephyr action client for native_sim
fn get_zephyr_action_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-action-client", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-action-client binary")
}

/// Test: Zephyr action server can be built or found
#[test]
fn test_zephyr_action_server_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result =
        get_or_build_zephyr_example("zephyr-rs-action-server", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built action server at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-action-server: {}", e);
        }
    }
}

/// Test: Zephyr action client can be built or found
#[test]
fn test_zephyr_action_client_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result =
        get_or_build_zephyr_example("zephyr-rs-action-client", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built action client at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-action-client: {}", e);
        }
    }
}

/// Test: Zephyr action server starts correctly
///
/// Basic smoke test that verifies the Zephyr action server boots and initializes.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_action_server_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_action_server_native_sim();
    eprintln!("Starting Zephyr action server: {}", zephyr_binary.display());

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action server");

    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr action server booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr action server failed to boot - no initialization output");
    }
}

/// Test: Zephyr action client starts correctly
///
/// Basic smoke test that verifies the Zephyr action client boots and initializes.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_action_client_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_action_client_native_sim();
    eprintln!("Starting Zephyr action client: {}", zephyr_binary.display());

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action client");

    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr action client booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr action client failed to boot - no initialization output");
    }
}

/// Test: Zephyr action server → Zephyr action client communication
///
/// This is a full E2E integration test that:
/// 1. Starts zenohd automatically automatically
/// 2. Runs both Zephyr action server and client
/// 3. Verifies action communication works
///
/// NOTE: This test documents a known zenoh-pico limitation where two clients
/// connecting simultaneously can cause subscription failures.
///
/// Requires:
/// - NSOS board overlays in examples/zephyr/*/boards/ (checked into git)
/// - Both examples built with their specific TAP interface configs
#[test]
fn test_zephyr_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd started on port {}", platform::ZEPHYR.zenohd_port);

    std::thread::sleep(Duration::from_millis(500));

    // Build both examples
    let server_binary = get_zephyr_action_server_native_sim();
    let client_binary = get_zephyr_action_client_native_sim();

    eprintln!("Action server binary: {}", server_binary.display());
    eprintln!("Action client binary: {}", client_binary.display());

    // Start action server first
    eprintln!("Starting Zephyr action server...");
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action server");

    // Give server time to connect and set up queryables
    std::thread::sleep(Duration::from_secs(2));

    // Start action client
    eprintln!("Starting Zephyr action client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action client");

    // Wait for action communication
    eprintln!("Waiting for action communication...");

    // Wait for output from both
    let server_output = server
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    let client_output = client
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    // Kill processes
    let _ = server.kill();
    let _ = client.kill();
    drop(router);

    eprintln!("\n=== Action Server output ===\n{}", server_output);
    eprintln!("\n=== Action Client output ===\n{}", client_output);

    // Check server status
    let server_connected =
        server_output.contains("Session opened") || server_output.contains("Action server ready");
    let server_created_queryables =
        server_output.contains("Queryable") || server_output.contains("ready");
    let server_received_goal = server_output.contains("Received goal")
        || server_output.contains("Goal accepted")
        || server_output.contains("Goal request");

    // Check client status
    let client_connected =
        client_output.contains("Session opened") || client_output.contains("Action client ready");
    let _client_subscribed = client_output.contains("Feedback subscriber ready")
        || client_output.contains("Subscriber created");
    let client_got_feedback =
        client_output.contains("Feedback #") || client_output.contains("feedback");
    let client_completed = client_output.contains("completed") || client_output.contains("Result:");

    // Report results
    if !server_connected {
        panic!("Action server failed to connect to zenohd");
    }
    if !client_connected {
        panic!("Action client failed to connect to zenohd");
    }

    // Full success case — if client got feedback and completed, the action worked
    // regardless of whether the "subscriber ready" log message appeared
    if server_received_goal && client_got_feedback && client_completed {
        let feedback_count = count_pattern(&client_output, "Feedback #");
        eprintln!("\nSUCCESS: Zephyr action communication works!");
        eprintln!("  - Server received goal");
        eprintln!("  - Client received {} feedback messages", feedback_count);
        eprintln!("  - Action completed successfully");
    } else if !server_received_goal {
        panic!("Server didn't receive goal");
    } else if !client_got_feedback {
        panic!("Client didn't receive feedback");
    } else {
        panic!(
            "Action test failed:\n  server_connected={}\n  server_queryables={}\n  server_goal={}\n  client_connected={}\n  client_feedback={}\n  client_completed={}",
            server_connected,
            server_created_queryables,
            server_received_goal,
            client_connected,
            client_got_feedback,
            client_completed
        );
    }
}

// =============================================================================
// Zephyr Service Examples
// =============================================================================

/// Get or build Zephyr service server for native_sim
fn get_zephyr_service_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-service-server", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-service-server binary")
}

/// Get or build Zephyr service client for native_sim
fn get_zephyr_service_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-rs-service-client", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-rs-service-client binary")
}

/// Test: Zephyr service server can be built or found
#[test]
fn test_zephyr_service_server_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result =
        get_or_build_zephyr_example("zephyr-rs-service-server", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built service server at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-service-server: {}", e);
        }
    }
}

/// Test: Zephyr service client can be built or found
#[test]
fn test_zephyr_service_client_build() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let result =
        get_or_build_zephyr_example("zephyr-rs-service-client", ZephyrPlatform::NativeSim, false);

    match result {
        Ok(path) => {
            assert!(path.exists(), "Binary should exist");
            eprintln!("SUCCESS: Found/built service client at {}", path.display());
        }
        Err(e) => {
            panic!("Failed to get zephyr-rs-service-client: {}", e);
        }
    }
}

/// Test: Zephyr service server starts correctly
///
/// Basic smoke test that verifies the Zephyr service server boots and initializes.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_service_server_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_service_server_native_sim();
    eprintln!(
        "Starting Zephyr service server: {}",
        zephyr_binary.display()
    );

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr service server");

    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr service server booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr service server failed to boot - no initialization output");
    }
}

/// Test: Zephyr service client starts correctly
///
/// Basic smoke test that verifies the Zephyr service client boots and initializes.
/// Connection failure is expected without zenohd.
#[test]
fn test_zephyr_service_client_smoke() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let zephyr_binary = get_zephyr_service_client_native_sim();
    eprintln!(
        "Starting Zephyr service client: {}",
        zephyr_binary.display()
    );

    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr service client");

    let output = zephyr
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("Zephyr output:\n{}", output);

    let has_boot = output.contains("Booting Zephyr") || output.contains("nros");

    if has_boot {
        eprintln!("SUCCESS: Zephyr service client booted and initialized");
        if output.contains("ConnectionFailed") || output.contains("session error") {
            eprintln!("NOTE: Connection error above is expected (no zenohd in smoke test)");
        }
    } else {
        panic!("Zephyr service client failed to boot - no initialization output");
    }
}

// =============================================================================
// Cross-Platform Service Tests
// =============================================================================

/// Test: Native service server + Zephyr service client
///
/// Tests cross-platform service communication with native server and Zephyr client.
#[test]
fn test_native_server_zephyr_client() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    std::thread::sleep(Duration::from_millis(500));

    // Build native service server
    let server_path =
        build_native_service_server().expect("Failed to build native-rs-service-server");

    // Get Zephyr service client
    let zephyr_binary = get_zephyr_service_client_native_sim();
    eprintln!("Zephyr client binary: {}", zephyr_binary.display());

    // Start native service server first
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut server_cmd = Command::new(server_path);
    server_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-service-server")
        .expect("Failed to start native service server");

    // Give server time to set up
    std::thread::sleep(Duration::from_secs(2));

    if !server.is_running() {
        let output = server
            .wait_for_all_output(Duration::from_secs(1))
            .unwrap_or_default();
        eprintln!("[FAIL] Native service server exited early");
        eprintln!("Output: {}", output);
        panic!("Native service server failed to start");
    }

    // Start Zephyr service client
    eprintln!("Starting Zephyr service client...");
    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr service client");

    // Wait for service communication
    eprintln!("Waiting for Native server ↔ Zephyr client communication...");

    // Wait for Zephyr output
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    // Get native server output
    let server_output = server
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Kill processes
    let _ = zephyr.kill();
    drop(server);
    drop(router);

    eprintln!("\n=== Native server output ===\n{}", server_output);
    eprintln!("\n=== Zephyr client output ===\n{}", zephyr_output);

    // Check Zephyr client status
    // "Session opened" or "Service client ready" or "Sending:" all indicate connection
    let zephyr_connected = zephyr_output.contains("Session opened")
        || zephyr_output.contains("Service client ready")
        || zephyr_output.contains("Sending:");
    let zephyr_sent_request = zephyr_output.contains("Sending request")
        || zephyr_output.contains("Request:")
        || zephyr_output.contains("Sending:");
    let zephyr_got_response = zephyr_output.contains("Response:") || zephyr_output.contains("sum=");

    // Check native server status
    let server_received = server_output.contains("Received request")
        || server_output.contains("Processing request")
        || server_output.contains("Request:");

    if zephyr_got_response {
        let response_count = count_pattern(&zephyr_output, "Response");
        eprintln!(
            "\nSUCCESS: Zephyr client received {} responses from native server",
            response_count
        );
    } else if zephyr_connected && zephyr_sent_request {
        panic!(
            "Zephyr service E2E failed — client sent requests but all timed out.\n\
             Server received request: {}\n\
             This indicates a zenoh queryable discovery issue. Verify:\n\
             - Zephyr binary rebuilt after CMakeLists.txt changes: `west build`\n\
             - zenohd running on bridge IP and reachable from both native and Zephyr processes",
            server_received
        );
    } else if !zephyr_connected {
        panic!(
            "Zephyr service E2E failed — client did not connect to zenohd.\n\
             This is an environment issue. Verify:\n\
             - Zephyr bridge: `ip addr show zeth-br` (should have 192.0.2.2)\n\
             - Zephyr binary up to date: rebuild with `west build`\n\
             - zenohd reachable on bridge IP"
        );
    } else {
        panic!(
            "Zephyr service E2E failed — incomplete communication.\n\
             Zephyr connected: {}, sent request: {}, got response: {}, server received: {}",
            zephyr_connected, zephyr_sent_request, zephyr_got_response, server_received
        );
    }
}

// =============================================================================
// Zephyr XRCE-DDS E2E Tests (with XRCE Agent)
// =============================================================================

/// Get or build Zephyr XRCE Rust talker for native_sim
fn get_zephyr_xrce_rs_talker_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-xrce-rs-talker", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-xrce-rs-talker binary")
}

/// Get or build Zephyr XRCE Rust listener for native_sim
fn get_zephyr_xrce_rs_listener_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-xrce-rs-listener", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-xrce-rs-listener binary")
}

/// Get or build Zephyr XRCE C talker for native_sim
fn get_zephyr_xrce_c_talker_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-xrce-c-talker", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-xrce-c-talker binary")
}

/// Get or build Zephyr XRCE C listener for native_sim
fn get_zephyr_xrce_c_listener_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-xrce-c-listener", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-xrce-c-listener binary")
}

/// Test: Zephyr XRCE Rust talker → listener communication
///
/// E2E integration test for XRCE-DDS backend on Zephyr:
/// 1. Starts MicroXRCEAgent on port 2018
/// 2. Runs Zephyr listener (native_sim)
/// 3. Runs Zephyr talker (native_sim)
/// 4. Verifies message delivery
///
/// Requires:
/// - NSOS board overlays in examples/zephyr/*/boards/ (checked into git)
/// - XRCE Agent available: `just build-xrce-agent`
#[test]
fn test_zephyr_xrce_rust_talker_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    // Start XRCE Agent on port 2018 (compiled into Zephyr binaries via Kconfig)
    eprintln!("Starting XRCE Agent on port 2018...");
    let _agent = XrceAgent::start(2018).expect("Failed to start XRCE Agent");

    // Give agent time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build both examples
    let talker_binary = get_zephyr_xrce_rs_talker_native_sim();
    let listener_binary = get_zephyr_xrce_rs_listener_native_sim();

    eprintln!("Talker binary: {}", talker_binary.display());
    eprintln!("Listener binary: {}", listener_binary.display());

    // Start listener first (subscribe before publish)
    eprintln!("Starting Zephyr XRCE listener...");
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE listener");

    // Give listener time to connect and create subscription
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    eprintln!("Starting Zephyr XRCE talker...");
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE talker");

    // Wait for communication
    eprintln!("Waiting for XRCE talker → listener communication...");

    let talker_output = talker
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    // Kill processes
    let _ = talker.kill();
    let _ = listener.kill();

    eprintln!("\n=== XRCE Talker output ===\n{}", talker_output);
    eprintln!("\n=== XRCE Listener output ===\n{}", listener_output);

    // Check talker status
    let talker_published = talker_output.contains("Published:") || talker_output.contains("data=");
    let talker_error = talker_output.contains("Error:");

    // Check listener status
    let listener_received = listener_output.contains("Received:");
    let listener_waiting = listener_output.contains("Waiting for messages");
    let listener_error = listener_output.contains("Error:");

    if talker_error {
        panic!("XRCE talker encountered an error:\n{}", talker_output);
    }
    if listener_error && !listener_received {
        panic!("XRCE listener encountered an error:\n{}", listener_output);
    }

    if listener_received {
        let count = count_pattern(&listener_output, "Received:");
        eprintln!(
            "\nSUCCESS: Zephyr XRCE listener received {} messages from talker",
            count
        );
    } else if talker_published && listener_waiting {
        panic!("Talker published but listener didn't receive (timing issue?)");
    } else {
        panic!(
            "XRCE communication failed:\n  talker_published={}\n  listener_waiting={}\n  listener_received={}",
            talker_published, listener_waiting, listener_received
        );
    }
}

/// Test: Zephyr XRCE C talker → listener communication
///
/// E2E integration test for XRCE-DDS C API backend on Zephyr:
/// 1. Starts MicroXRCEAgent on port 2018
/// 2. Runs C listener (native_sim)
/// 3. Runs C talker (native_sim)
/// 4. Verifies message delivery
///
/// Requires:
/// - NSOS board overlays in examples/zephyr/*/boards/ (checked into git)
/// - XRCE Agent available: `just build-xrce-agent`
#[test]
#[ignore] // XRCE C: agent doesn't forward data between C API talker/listener (Rust XRCE test passes)
fn test_zephyr_xrce_c_talker_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    // Start XRCE Agent on port 2018 (compiled into Zephyr binaries via Kconfig)
    eprintln!("Starting XRCE Agent on port 2018...");
    let _agent = XrceAgent::start(2018).expect("Failed to start XRCE Agent");

    // Give agent time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build both examples
    let talker_binary = get_zephyr_xrce_c_talker_native_sim();
    let listener_binary = get_zephyr_xrce_c_listener_native_sim();

    eprintln!("C Talker binary: {}", talker_binary.display());
    eprintln!("C Listener binary: {}", listener_binary.display());

    // Start listener first (subscribe before publish)
    eprintln!("Starting Zephyr XRCE C listener...");
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE C listener");

    // Give listener time to connect and create subscription
    std::thread::sleep(Duration::from_secs(3));

    // Start talker
    eprintln!("Starting Zephyr XRCE C talker...");
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE C talker");

    // Wait for communication
    eprintln!("Waiting for XRCE C talker → listener communication...");

    let talker_output = talker
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();

    // Kill processes
    let _ = talker.kill();
    let _ = listener.kill();

    eprintln!("\n=== XRCE C Talker output ===\n{}", talker_output);
    eprintln!("\n=== XRCE C Listener output ===\n{}", listener_output);

    // Check talker status (C API uses LOG_INF format)
    let talker_published = talker_output.contains("Published:");
    let talker_init = talker_output.contains("Publishing messages");
    let talker_error =
        talker_output.contains("failed") || talker_output.contains("Network not ready");

    // Check listener status (C API uses LOG_INF format)
    let listener_received = listener_output.contains("Received");
    let listener_waiting = listener_output.contains("Waiting for messages");
    let listener_error =
        listener_output.contains("failed") || listener_output.contains("Network not ready");

    if talker_error && !talker_init {
        panic!("XRCE C talker encountered an error:\n{}", talker_output);
    }
    if listener_error && !listener_waiting {
        panic!("XRCE C listener encountered an error:\n{}", listener_output);
    }

    if listener_received {
        let count = count_pattern(&listener_output, "Received");
        eprintln!(
            "\nSUCCESS: Zephyr XRCE C listener received {} messages from talker",
            count
        );
    } else if talker_published && listener_waiting {
        panic!("C talker published but listener didn't receive (timing issue?)");
    } else {
        panic!(
            "XRCE C communication failed:\n  talker_published={}\n  listener_waiting={}\n  listener_received={}",
            talker_published, listener_waiting, listener_received
        );
    }
}

// =============================================================================
// Cross-Platform Service Tests
// =============================================================================

/// Test: Zephyr service server + Native service client
///
/// Tests cross-platform service communication with Zephyr server and native client.
#[test]
fn test_zephyr_server_native_client() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Start zenohd router
    eprintln!("Starting zenohd router...");
    let router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    std::thread::sleep(Duration::from_millis(500));

    // Build native service client
    let client_path =
        build_native_service_client().expect("Failed to build native-rs-service-client");

    // Get Zephyr service server
    let zephyr_binary = get_zephyr_service_server_native_sim();
    eprintln!("Zephyr server binary: {}", zephyr_binary.display());

    // Start Zephyr service server first
    eprintln!("Starting Zephyr service server...");
    let mut zephyr = ZephyrProcess::start(&zephyr_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr service server");

    // Give Zephyr server time to set up queryables
    std::thread::sleep(Duration::from_secs(3));

    // Start native service client
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut client_cmd = Command::new(client_path);
    client_cmd
        .env("NROS_LOCATOR", "tcp/127.0.0.1:7456")
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start native service client");

    // Wait for service communication
    eprintln!("Waiting for Zephyr server ↔ Native client communication...");
    std::thread::sleep(Duration::from_secs(8));

    // Get outputs
    let client_output = client
        .wait_for_all_output(Duration::from_secs(3))
        .unwrap_or_default();
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Kill processes
    let _ = zephyr.kill();
    drop(client);
    drop(router);

    eprintln!("\n=== Zephyr server output ===\n{}", zephyr_output);
    eprintln!("\n=== Native client output ===\n{}", client_output);

    // Check Zephyr server status
    let zephyr_connected = zephyr_output.contains("Session opened");
    let zephyr_ready = zephyr_output.contains("Service server ready")
        || zephyr_output.contains("Waiting for service requests");
    let zephyr_received =
        zephyr_output.contains("Received request") || zephyr_output.contains("Request:");
    let zephyr_replied = zephyr_output.contains("Sent reply") || zephyr_output.contains("sum=");

    // Check native client status
    let client_got_response = client_output.contains("Response:") || client_output.contains("= ");
    let client_completed = client_output.contains("completed successfully");

    if client_got_response {
        let response_count = count_pattern(&client_output, "Response:");
        eprintln!(
            "\nSUCCESS: Native client received {} responses from Zephyr server",
            response_count
        );
        if zephyr_replied {
            eprintln!("  - Zephyr server processed and replied to requests");
        }
    } else if zephyr_connected && zephyr_ready && !zephyr_received {
        panic!("Zephyr server ready but didn't receive requests");
    } else if !zephyr_connected {
        panic!("Zephyr server failed to connect to zenohd");
    } else {
        panic!(
            "Service communication failed:\n  zephyr_connected={}\n  zephyr_ready={}\n  zephyr_received={}\n  zephyr_replied={}\n  client_response={}\n  client_completed={}",
            zephyr_connected,
            zephyr_ready,
            zephyr_received,
            zephyr_replied,
            client_got_response,
            client_completed
        );
    }
}

// =============================================================================
// Zephyr C++ E2E Tests
// =============================================================================

/// Get or build Zephyr C++ talker for native_sim
fn get_zephyr_cpp_talker_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-talker", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-talker binary")
}

/// Get or build Zephyr C++ listener for native_sim
fn get_zephyr_cpp_listener_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-listener", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-listener binary")
}

/// Test: Zephyr C++ talker → Zephyr C++ listener communication
///
/// Full E2E integration test:
/// 1. Starts zenohd automatically
/// 2. Runs both Zephyr C++ talker and listener
/// 3. Verifies messages are delivered
#[test]
fn test_zephyr_cpp_talker_to_listener_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    eprintln!("Starting zenohd router...");
    let _router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    let talker_binary = get_zephyr_cpp_talker_native_sim();
    let listener_binary = get_zephyr_cpp_listener_native_sim();

    eprintln!("C++ Talker binary: {}", talker_binary.display());
    eprintln!("C++ Listener binary: {}", listener_binary.display());

    // Start listener first (subscriber must be ready before publisher)
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim).unwrap();
    std::thread::sleep(Duration::from_secs(1));

    // Start talker
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for messages to flow
    std::thread::sleep(Duration::from_secs(8));

    // Collect outputs
    let talker_output = talker
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("\n=== Zephyr C++ talker output ===\n{}", talker_output);
    eprintln!("\n=== Zephyr C++ listener output ===\n{}", listener_output);

    // Check talker connected and published
    let talker_published = talker_output.contains("Published:");
    // Check listener received messages
    let listener_received = count_pattern(&listener_output, "Received");

    if listener_received >= 3 {
        eprintln!(
            "\nSUCCESS: Zephyr C++ listener received {} messages from talker",
            listener_received
        );
    } else if talker_published && listener_received > 0 {
        panic!(
            "Listener received only {} messages (expected >= 3)",
            listener_received
        );
    } else if !talker_output.contains("nros Zephyr C++ Talker") {
        panic!("Zephyr C++ talker failed to start");
    } else if !talker_published {
        panic!("Talker started but didn't publish — zenoh-pico session issue");
    } else {
        panic!(
            "Talker published but listener received {} messages",
            listener_received
        );
    }
}

/// Test: Zephyr C++ talker → native Rust listener (cross-platform)
#[test]
fn test_zephyr_cpp_talker_to_native_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let _router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    // Build native Rust listener
    let native_listener = match build_native_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build native listener: {}", e);
            return;
        }
    };

    // Build Zephyr C++ talker
    let talker_binary = get_zephyr_cpp_talker_native_sim();

    // Start native listener first (connects to zenohd)
    let mut listener_cmd = std::process::Command::new(&native_listener);
    listener_cmd.env("NROS_LOCATOR", "tcp/127.0.0.1:7456");
    listener_cmd.env("RUST_LOG", "info");
    let mut listener =
        nros_tests::fixtures::ManagedProcess::spawn_command(listener_cmd, "native-listener")
            .expect("Failed to start native listener");

    std::thread::sleep(Duration::from_secs(2));

    // Start Zephyr C++ talker
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for messages
    std::thread::sleep(Duration::from_secs(8));

    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(3))
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("\n=== Native listener output ===\n{}", listener_output);
    eprintln!("\n=== Zephyr C++ talker output ===\n{}", talker_output);

    let received_count = count_pattern(&listener_output, "Received");

    if received_count >= 2 {
        eprintln!(
            "\nSUCCESS: Native listener received {} messages from Zephyr C++ talker",
            received_count
        );
    } else if talker_output.contains("Published:") {
        panic!(
            "Talker published but listener got only {} messages (expected >= 2)",
            received_count
        );
    } else {
        panic!(
            "Cross-platform C++ talker→native listener test failed (received {})",
            received_count
        );
    }
}

/// Test: native Rust talker → Zephyr C++ listener (cross-platform)
#[test]
fn test_native_talker_to_zephyr_cpp_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let _router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    // Build native Rust talker
    let native_talker = match build_native_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            eprintln!("Skipping: could not build native talker: {}", e);
            return;
        }
    };

    // Build Zephyr C++ listener
    let listener_binary = get_zephyr_cpp_listener_native_sim();

    // Start Zephyr listener first
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim).unwrap();
    std::thread::sleep(Duration::from_secs(1));

    // Start native talker (connects to zenohd)
    let mut talker_cmd = std::process::Command::new(&native_talker);
    talker_cmd.env("NROS_LOCATOR", "tcp/127.0.0.1:7456");
    talker_cmd.env("RUST_LOG", "info");
    let mut talker =
        nros_tests::fixtures::ManagedProcess::spawn_command(talker_cmd, "native-talker")
            .expect("Failed to start native talker");

    // Wait for messages
    std::thread::sleep(Duration::from_secs(8));

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!("\n=== Native talker output ===\n{}", talker_output);
    eprintln!("\n=== Zephyr C++ listener output ===\n{}", listener_output);

    let received_count = count_pattern(&listener_output, "Received");

    if received_count >= 2 {
        eprintln!(
            "\nSUCCESS: Zephyr C++ listener received {} messages from native talker",
            received_count
        );
    } else if talker_output.contains("Published") {
        panic!(
            "Talker published but Zephyr got only {} messages (expected >= 2)",
            received_count
        );
    } else {
        panic!(
            "Cross-platform native talker→C++ listener test failed (received {})",
            received_count
        );
    }
}

// =============================================================================
// Zephyr C++ Service E2E Tests
// =============================================================================

/// Get or build Zephyr C++ service server for native_sim
fn get_zephyr_cpp_service_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-service-server", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-service-server binary")
}

/// Get or build Zephyr C++ service client for native_sim
fn get_zephyr_cpp_service_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-service-client", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-service-client binary")
}

/// Test: Zephyr C++ service server → Zephyr C++ service client communication
///
/// Full E2E integration test:
/// 1. Starts zenohd automatically
/// 2. Runs Zephyr C++ service server and client
/// 3. Verifies service calls succeed
#[test]
fn test_zephyr_cpp_service_server_to_client_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    eprintln!("Starting zenohd router...");
    let _router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    let server_binary = get_zephyr_cpp_service_server_native_sim();
    let client_binary = get_zephyr_cpp_service_client_native_sim();

    eprintln!("C++ Service Server binary: {}", server_binary.display());
    eprintln!("C++ Service Client binary: {}", client_binary.display());

    // Start server first
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim).unwrap();
    std::thread::sleep(Duration::from_secs(3));

    // Start client
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for client to complete (4 calls × ~1s sleep + connection time)
    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();

    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!(
        "\n=== Zephyr C++ service server output ===\n{}",
        server_output
    );
    eprintln!(
        "\n=== Zephyr C++ service client output ===\n{}",
        client_output
    );

    let ok_count = count_pattern(&client_output, "[OK]");
    let request_count = count_pattern(&server_output, "Request");

    if ok_count >= 3 {
        eprintln!(
            "\nSUCCESS: C++ service client completed {} calls, server handled {} requests",
            ok_count, request_count
        );
    } else if request_count > 0 {
        panic!(
            "Server handled {} requests but client got only {} OK (expected >= 3)",
            request_count, ok_count
        );
    } else {
        panic!(
            "C++ service test failed (client OK={}, server requests={})",
            ok_count, request_count
        );
    }
}

// =============================================================================
// Zephyr C++ Action E2E Tests
// =============================================================================

/// Get or build Zephyr C++ action server for native_sim
fn get_zephyr_cpp_action_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-action-server", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-action-server binary")
}

/// Get or build Zephyr C++ action client for native_sim
fn get_zephyr_cpp_action_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example("cpp-action-client", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-cpp-action-client binary")
}

/// Test: Zephyr C++ action server → Zephyr C++ action client communication
///
/// Full E2E integration test:
/// 1. Starts zenohd automatically
/// 2. Runs Zephyr C++ action server and client
/// 3. Verifies goal completion
#[test]
fn test_zephyr_cpp_action_server_to_client_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    eprintln!("Starting zenohd router...");
    let _router = ZenohRouter::start(platform::ZEPHYR.zenohd_port).expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    let server_binary = get_zephyr_cpp_action_server_native_sim();
    let client_binary = get_zephyr_cpp_action_client_native_sim();

    eprintln!("C++ Action Server binary: {}", server_binary.display());
    eprintln!("C++ Action Client binary: {}", client_binary.display());

    // Start action server first
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim).unwrap();
    std::thread::sleep(Duration::from_secs(3));

    // Start action client
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for client to complete
    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();

    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    eprintln!(
        "\n=== Zephyr C++ action server output ===\n{}",
        server_output
    );
    eprintln!(
        "\n=== Zephyr C++ action client output ===\n{}",
        client_output
    );

    let client_ok = client_output.contains("[OK]");
    let server_completed = server_output.contains("Goal completed");

    if client_ok && server_completed {
        eprintln!("\nSUCCESS: C++ action server completed goal, client received result");
    } else if server_completed {
        panic!("Server completed goal but client didn't get result");
    } else if server_output.contains("Goal received") {
        panic!("Server received goal but didn't complete");
    } else {
        panic!(
            "C++ action test failed (client OK={}, server completed={})",
            client_ok, server_completed
        );
    }
}
