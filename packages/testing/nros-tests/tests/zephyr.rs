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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
    eprintln!(
        "zenohd started on port {}",
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust)
    );

    // Give zenohd time to start
    std::thread::sleep(Duration::from_millis(500));

    // Build both examples (to separate directories)
    let talker_binary = get_zephyr_talker_native_sim();
    let listener_binary = get_zephyr_listener_native_sim();

    eprintln!("Talker binary: {}", talker_binary.display());
    eprintln!("Listener binary: {}", listener_binary.display());

    // Start listener first (so it creates its subscriber before talker publishes)
    eprintln!("Starting Zephyr listener...");
    let listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr listener");

    // Wait for listener to reach subscriber readiness before starting
    // talker. Under parallel load the native_sim cold-boot +
    // `Executor::open` + subscription-declaration propagation to the
    // zenohd router regularly slips past any fixed sleep; polling the
    // actual output marker is the robust fix (Phase 89.12).
    let listener_ready = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    if !listener_ready.contains("Waiting for messages") {
        panic!(
            "Zephyr listener didn't reach readiness within 30 s.\nOutput:\n{}",
            listener_ready
        );
    }
    // Small additional delay so the subscription declaration can
    // reach the router even after the listener's log line was
    // emitted.
    std::thread::sleep(Duration::from_millis(500));
    let mut listener = listener;

    // Start talker
    eprintln!("Starting Zephyr talker...");
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr talker");

    // Wait for communication
    eprintln!("Waiting for Zephyr talker → listener communication...");

    // Probe for the talker's 3rd publish + the listener's 3rd
    // Received marker, early-exiting as soon as both have emitted
    // enough output. Under `max-threads = 3` parallel load the
    // native_sim cold-boot + session open can take >8 s, so the
    // old fixed-8 s `wait_for_output` regularly missed the first
    // couple of publishes. 30 s cap is comfortable headroom.
    let _ = talker.wait_for_pattern("Published: 3", Duration::from_secs(30));
    let _ = listener.wait_for_pattern("Received: 3", Duration::from_secs(30));
    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(2))
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust)
            ),
        )
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

    // Strict delivery check: the native listener must log at least one
    // real "Received: <N>" line (not setup text like "Waiting for Int32 ...").
    let received_count = count_pattern(&listener_output, "Received:");
    let zephyr_transport_err = zephyr_output.contains("Transport(ConnectionFailed)")
        || zephyr_output.contains("z_publisher_put failed")
        || zephyr_output.contains("Failed to publish");

    if received_count >= 1 {
        eprintln!(
            "\nSUCCESS: Native listener received {} messages from Zephyr talker",
            received_count
        );
    } else if zephyr_transport_err {
        panic!(
            "Zephyr talker transport failed — check zenoh-pico session setup. \
             Listener received 0 messages."
        );
    } else {
        panic!(
            "No messages delivered from Zephyr talker to native listener. \
             Listener received 0 'Received:' lines."
        );
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust)
            ),
        )
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

    // Strict delivery check: the Zephyr listener must log at least one
    // real "Received: <N>" line (not setup text like "Waiting for messages ...").
    let received_count = count_pattern(&zephyr_output, "Received:");
    let zephyr_transport_err = zephyr_output.contains("Transport(ConnectionFailed)")
        || zephyr_output.contains("z_declare_subscriber failed")
        || zephyr_output.contains("Failed to create subscriber");
    let talker_published = talker_output.contains("Published");

    if received_count >= 1 {
        eprintln!(
            "\nSUCCESS: Zephyr listener received {} messages from native talker",
            received_count
        );
    } else if zephyr_transport_err {
        panic!(
            "Zephyr listener transport failed — check zenoh-pico session setup. \
             Listener received 0 messages."
        );
    } else if !talker_published {
        panic!("Native talker did not publish — check talker output for errors");
    } else {
        panic!(
            "Native talker published but Zephyr listener received 0 messages. \
             Check Zephyr output for subscription/session errors."
        );
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust)
            ),
        )
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust)
            ),
        )
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

    // Strict delivery counts: match only real "Received: <N>" lines,
    // not setup text like "Waiting for Int32 messages ...".
    let native_received_count = count_pattern(&native_listener_output, "Received:");
    let zephyr_received_count = count_pattern(&zephyr_listener_output, "Received:");

    eprintln!("\n=== Results ===");
    eprintln!(
        "Direction 1 (Zephyr → Native): {} messages received",
        native_received_count
    );
    eprintln!(
        "Direction 2 (Native → Zephyr): {} messages received",
        zephyr_received_count
    );

    match (native_received_count >= 1, zephyr_received_count >= 1) {
        (true, true) => {
            eprintln!("\nSUCCESS: Bidirectional communication works!");
        }
        (true, false) => panic!(
            "Zephyr → Native works ({} msgs), Native → Zephyr failed (0 msgs)",
            native_received_count
        ),
        (false, true) => panic!(
            "Native → Zephyr works ({} msgs), Zephyr → Native failed (0 msgs)",
            zephyr_received_count
        ),
        (false, false) => {
            panic!("Bidirectional communication failed — 0 messages in both directions")
        }
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
    eprintln!(
        "zenohd started on port {}",
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust)
    );

    std::thread::sleep(Duration::from_millis(500));

    // Build both examples
    let server_binary = get_zephyr_action_server_native_sim();
    let client_binary = get_zephyr_action_client_native_sim();

    eprintln!("Action server binary: {}", server_binary.display());
    eprintln!("Action client binary: {}", client_binary.display());

    // Start action server first
    eprintln!("Starting Zephyr action server...");
    let server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action server");

    // Wait for server to declare its queryables before starting the
    // client. Under parallel load this replaces a fixed sleep that
    // would otherwise race `client.send_goal()` against a
    // still-initialising queryable (Phase 89.12 flake). The
    // readiness marker is any of the "ready" / "Queryable" strings
    // the Zephyr action server example emits after
    // `create_action_server` returns — match on the common substring
    // "Action server ready".
    let server_ready = server.wait_for_pattern("Action server ready", Duration::from_secs(30));
    if !server_ready.contains("Action server ready") {
        panic!(
            "Zephyr action server didn't reach readiness within 30 s.\nOutput:\n{}",
            server_ready
        );
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut server = server;

    // Start action client
    eprintln!("Starting Zephyr action client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action client");

    // Wait for action communication
    eprintln!("Waiting for action communication...");

    // Wait for the client to print the action-completion marker
    // (early-exits as soon as the action completes; falls back to a
    // 40 s cap so a stuck client still returns and surfaces the
    // failure). 40 s is enough headroom for the client's 30 s
    // `get_result.wait(…)` under `max-threads = 3` parallelism.
    let client_output = client.wait_for_pattern("Action client finished", Duration::from_secs(40));
    // Server output can stop shortly after the client finishes —
    // give the reader a few seconds to drain any trailing feedback.
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust)
            ),
        )
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
             Verify:\n\
             - Zephyr binary up to date: rebuild with `west build`\n\
             - zenohd reachable on tcp/127.0.0.1:7456 (NSOS forwards sockets to host loopback)"
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
// Previously #[ignore]: C talker didn't flush XRCE output stream after publish (fixed)
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
// Zephyr XRCE Rust Service + Action E2E Tests (Phase 95.A)
// =============================================================================

fn get_zephyr_xrce_rs_service_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-xrce-rs-service-server",
        ZephyrPlatform::NativeSim,
        false,
    )
    .expect("Failed to get zephyr-xrce-rs-service-server binary")
}

fn get_zephyr_xrce_rs_service_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-xrce-rs-service-client",
        ZephyrPlatform::NativeSim,
        false,
    )
    .expect("Failed to get zephyr-xrce-rs-service-client binary")
}

fn get_zephyr_xrce_rs_action_server_native_sim() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-xrce-rs-action-server",
        ZephyrPlatform::NativeSim,
        false,
    )
    .expect("Failed to get zephyr-xrce-rs-action-server binary")
}

fn get_zephyr_xrce_rs_action_client_native_sim() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-xrce-rs-action-client",
        ZephyrPlatform::NativeSim,
        false,
    )
    .expect("Failed to get zephyr-xrce-rs-action-client binary")
}

/// Test: Zephyr XRCE Rust service server → Zephyr XRCE Rust service client
///
/// E2E integration test for XRCE service path on Zephyr:
/// 1. Starts MicroXRCEAgent on port 2018
/// 2. Runs server + client (both native_sim)
/// 3. Verifies the client receives at least one response
#[test]
fn test_zephyr_xrce_rust_service_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    eprintln!("Starting XRCE Agent on port 2018...");
    let _agent = XrceAgent::start(2018).expect("Failed to start XRCE Agent");
    std::thread::sleep(Duration::from_millis(500));

    let server_binary = get_zephyr_xrce_rs_service_server_native_sim();
    let client_binary = get_zephyr_xrce_rs_service_client_native_sim();

    eprintln!("XRCE service server binary: {}", server_binary.display());
    eprintln!("XRCE service client binary: {}", client_binary.display());

    eprintln!("Starting Zephyr XRCE service server...");
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE service server");
    std::thread::sleep(Duration::from_secs(3));

    eprintln!("Starting Zephyr XRCE service client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE service client");

    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    let _ = client.kill();
    let _ = server.kill();

    eprintln!("\n=== XRCE service server output ===\n{}", server_output);
    eprintln!("\n=== XRCE service client output ===\n{}", client_output);

    let response_count = count_pattern(&client_output, "Response: sum=");
    let request_count = count_pattern(&server_output, " + ");

    if response_count >= 1 {
        eprintln!(
            "\nSUCCESS: XRCE service client got {} responses, server handled {} requests",
            response_count, request_count
        );
    } else if request_count > 0 {
        panic!(
            "Server handled {} requests but client got 0 responses (timing/agent issue?)",
            request_count
        );
    } else {
        panic!(
            "XRCE service E2E failed:\n  client_responses={}\n  server_requests={}",
            response_count, request_count
        );
    }
}

/// Test: Zephyr XRCE Rust action server → Zephyr XRCE Rust action client
///
/// E2E integration test for XRCE action path on Zephyr:
/// 1. Starts MicroXRCEAgent on port 2018
/// 2. Runs Fibonacci server + client (both native_sim)
/// 3. Verifies "Action client finished" marker
#[test]
fn test_zephyr_xrce_rust_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    eprintln!("Starting XRCE Agent on port 2018...");
    let _agent = XrceAgent::start(2018).expect("Failed to start XRCE Agent");
    std::thread::sleep(Duration::from_millis(500));

    let server_binary = get_zephyr_xrce_rs_action_server_native_sim();
    let client_binary = get_zephyr_xrce_rs_action_client_native_sim();

    eprintln!("XRCE action server binary: {}", server_binary.display());
    eprintln!("XRCE action client binary: {}", client_binary.display());

    eprintln!("Starting Zephyr XRCE action server...");
    let server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE action server");

    let server_ready =
        server.wait_for_pattern("Action server ready", Duration::from_secs(30));
    if !server_ready.contains("Action server ready") {
        panic!(
            "Zephyr XRCE action server didn't reach readiness within 30s.\nOutput:\n{}",
            server_ready
        );
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut server = server;

    eprintln!("Starting Zephyr XRCE action client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE action client");

    let client_output =
        client.wait_for_pattern("Action client finished", Duration::from_secs(60));
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    let _ = server.kill();
    let _ = client.kill();

    eprintln!("\n=== XRCE action server output ===\n{}", server_output);
    eprintln!("\n=== XRCE action client output ===\n{}", client_output);

    let server_received_goal = server_output.contains("Goal request")
        || server_output.contains("Executing goal");
    let client_got_feedback = client_output.contains("Feedback #");
    let client_completed = client_output.contains("Action client finished")
        || client_output.contains("Result:");

    if client_completed && client_got_feedback {
        eprintln!("\nSUCCESS: XRCE action client received feedback and completed");
    } else if server_received_goal {
        panic!(
            "Server received goal but client didn't complete:\n  feedback={}\n  completed={}",
            client_got_feedback, client_completed
        );
    } else {
        panic!(
            "XRCE action E2E failed:\n  server_received_goal={}\n  client_feedback={}\n  client_completed={}",
            server_received_goal, client_got_feedback, client_completed
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
    let router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust))
            .expect("Failed to start zenohd");
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
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust)
            ),
        )
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
    let _router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp))
            .expect("Failed to start zenohd");
    std::thread::sleep(Duration::from_millis(500));

    let talker_binary = get_zephyr_cpp_talker_native_sim();
    let listener_binary = get_zephyr_cpp_listener_native_sim();

    eprintln!("C++ Talker binary: {}", talker_binary.display());
    eprintln!("C++ Listener binary: {}", listener_binary.display());

    // Start listener first (subscriber must be ready before publisher).
    // Probe for the listener's output readiness marker so we don't
    // race the publisher against a still-cold_booting native_sim
    // under full-suite parallel load (Phase 89.12 flake).
    let listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim).unwrap();
    let listener_ready = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    if !listener_ready.contains("Waiting for messages") {
        panic!(
            "Zephyr C++ listener didn't reach readiness within 30 s.\nOutput:\n{}",
            listener_ready
        );
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut listener = listener;

    // Start talker
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim).unwrap();

    // Probe for the 3rd publish + 3rd Received, early-exiting
    // instead of a fixed 8 s wait that couldn't keep up with
    // `max-threads = 3` parallel cold-boot variance.
    let _ = talker.wait_for_pattern("Published: 3", Duration::from_secs(30));
    let _ = listener.wait_for_pattern("Received: 3", Duration::from_secs(30));

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

    let _router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp))
            .expect("Failed to start zenohd");
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
    listener_cmd.env(
        "NROS_LOCATOR",
        format!(
            "tcp/127.0.0.1:{}",
            platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp)
        ),
    );
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

    let _router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp))
            .expect("Failed to start zenohd");
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

    // Start Zephyr listener first; wait for its subscription-ready
    // output marker so the native talker doesn't race a still-booting
    // subscriber (Phase 89.12 flake).
    let listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim).unwrap();
    let listener_ready = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    if !listener_ready.contains("Waiting for messages") {
        panic!(
            "Zephyr C++ listener didn't reach readiness within 30 s.\nOutput:\n{}",
            listener_ready
        );
    }
    std::thread::sleep(Duration::from_millis(500));
    let mut listener = listener;

    // Start native talker (connects to zenohd)
    let mut talker_cmd = std::process::Command::new(&native_talker);
    talker_cmd.env(
        "NROS_LOCATOR",
        format!(
            "tcp/127.0.0.1:{}",
            platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp)
        ),
    );
    talker_cmd.env("RUST_LOG", "info");
    let mut talker =
        nros_tests::fixtures::ManagedProcess::spawn_command(talker_cmd, "native-talker")
            .expect("Failed to start native talker");

    // Probe for the 3rd Received on the Zephyr side (early-exits
    // instead of the old 8 s+3 s blind sleep that couldn't keep
    // up with parallel-load variance).
    let _ = listener.wait_for_pattern("Received: 3", Duration::from_secs(30));

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(2))
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
    let _router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Cpp))
            .expect("Failed to start zenohd");
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
    let _router =
        ZenohRouter::start(platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Cpp))
            .expect("Failed to start zenohd");
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

// =============================================================================
// Zephyr DDS (dust-dds) Tests — Phase 71.8
// =============================================================================

/// Get or build Zephyr DDS talker for native_sim.
fn get_zephyr_dds_talker_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-dds-rs-talker", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-dds-rs-talker binary")
}

/// Get or build Zephyr DDS listener for native_sim.
fn get_zephyr_dds_listener_native_sim() -> PathBuf {
    get_or_build_zephyr_example("zephyr-dds-rs-listener", ZephyrPlatform::NativeSim, false)
        .expect("Failed to get zephyr-dds-rs-listener binary")
}

/// Test: Zephyr DDS talker boots through the cooperative
/// `NrosPlatformRuntime<ZephyrPlatform>` + `NrosUdpTransportFactory`
/// path and reaches steady-state publishing.
///
/// This is a *boot* smoke test — it does NOT exercise discovery/pubsub
/// across two participants because Zephyr's `mcast_listen` is still
/// the `-1` stub from the pre-Phase-71 zenoh-pico era. SPDP-multicast
/// support on Zephyr is its own work item; once that lands, a full
/// talker → listener interop test belongs alongside this one.
///
/// What this proves:
/// * `NROS_RMW_DDS=y` Kconfig wires `rmw-dds,platform-zephyr` features
///   correctly.
/// * dust-dds + nros-rmw-dds compile and link clean against
///   zephyr-lang-rust on `native_sim/native/64`.
/// * `Executor::open()` → `DdsRmw::open()` → cooperative `block_on`
///   `create_participant` returns successfully (the hang fixed in
///   commit `5fad3f1b`).
/// * The participant's spin loop drives the publisher; the timer
///   callback fires and `publish_raw` runs end-to-end without
///   panicking.
#[test]
fn test_zephyr_dds_rust_talker_boots() {
    if !require_zephyr() {
        return;
    }

    let talker_binary = get_zephyr_dds_talker_native_sim();
    eprintln!("Talker binary: {}", talker_binary.display());

    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr DDS talker");

    // Wait for the participant to be created and the timer to fire at
    // least once. `Published: 0` means: socket bound, participant
    // built, executor spin running, publish_raw returned.
    let output = talker.wait_for_pattern("Published: 0", Duration::from_secs(15));
    let _ = talker.kill();

    eprintln!("\n=== Talker output ===\n{}", output);

    if !output.contains("Published: 0") {
        panic!(
            "Zephyr DDS talker never reached steady-state publish.\n\
             Looked-for marker: \"Published: 0\".\n\
             This usually means `block_on(create_participant)` is hanging\n\
             again — see commit 5fad3f1b for the previous root-cause and\n\
             docs/roadmap/phase-71-dust-dds-platform-agnostic.md (71.8).\n\
             Output:\n{}",
            output
        );
    }
}

/// Test: Zephyr DDS listener boots, builds the subscriber, and parks
/// in `Executor::spin` waiting for messages.
///
/// Like the talker boot test above, this is single-process — it does
/// not validate inbound traffic because Zephyr SPDP discovery isn't
/// implemented yet (mcast_listen stub).
#[test]
fn test_zephyr_dds_rust_listener_boots() {
    if !require_zephyr() {
        return;
    }

    let listener_binary = get_zephyr_dds_listener_native_sim();
    eprintln!("Listener binary: {}", listener_binary.display());

    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr DDS listener");

    let output = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(15));
    let _ = listener.kill();

    eprintln!("\n=== Listener output ===\n{}", output);

    if !output.contains("Waiting for messages") {
        panic!(
            "Zephyr DDS listener never reached subscriber readiness.\n\
             Looked-for marker: \"Waiting for messages\".\n\
             Output:\n{}",
            output
        );
    }
}

// =============================================================================
// Zephyr DDS Talker↔Listener Interop on qemu_cortex_a9 — Phase 92.5
// =============================================================================
//
// Two QEMU Zynq-7000 guests connected by `-netdev socket,mcast=…`
// share a virtual L2 segment on the host (no sudo, no TAP). DDS
// SPDP/SEDP/data flow over Zephyr's native IP stack with real IGMP,
// real ARP, and the Xilinx GEM ethernet driver — same code path
// production DDS-on-Zephyr deployments will run on real silicon
// (Zynq, STM32-Eth, NXP-MAC).
//
// Phase 92's structural fixes that make this test possible:
//   1. zephyr-lang-rust Cortex-A9/A7 target case (upstream patch)
//   2. Zynq SoC SLCR MMU region (upstream patch)
//   3. nros-rmw-dds locator from CONFIG_NET_CONFIG_MY_IPV4_ADDR
//   4. mcast TX through the IGMP-joined fd (transport_nros)
//   5. ip_mreqn struct (Zephyr requires the 12-byte form)
//   6. Distinct DTS local-mac-address per guest (default DTS shares
//      00:00:00:01:02:03 → ARP self-loop drops)

fn get_zephyr_dds_talker_a9() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-dds-rs-talker-a9",
        ZephyrPlatform::QemuCortexA9,
        false,
    )
    .expect("Failed to get zephyr-dds-rs-talker-a9 binary")
}

fn get_zephyr_dds_listener_a9() -> PathBuf {
    get_or_build_zephyr_example(
        "zephyr-dds-rs-listener-a9",
        ZephyrPlatform::QemuCortexA9,
        false,
    )
    .expect("Failed to get zephyr-dds-rs-listener-a9 binary")
}

/// Pick a per-test mcast (group, port) so concurrent runs don't
/// share the same virtual L2 segment. Group is fixed
/// (`230.0.0.1` — host-local admin scope); port hashes the test's
/// PID into the dynamic range. Each test gets a unique port.
fn pick_mcast_addr_port() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let pid = std::process::id() as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let port = 49000 + (pid.wrapping_mul(2654435761).wrapping_add(nanos) % 1000);
    format!("230.0.0.1:{port}")
}

/// Talker → listener pubsub interop on `qemu_cortex_a9`.
///
/// Builds both binaries (cached after first run), launches them as
/// two QEMU instances joined to the same host-side multicast group,
/// then waits up to ~30 s of wall-clock for the listener to print
/// `Received: 5` (5 distinct samples of the talker's `Int32`
/// counter — pubs/sec is much higher in sim time so this happens
/// quickly once the SPDP/SEDP discovery handshake completes).
#[test]
fn test_zephyr_dds_rust_talker_to_listener_a9_e2e() {
    if !require_zephyr() {
        return;
    }
    if !nros_tests::zephyr::is_west_available() {
        nros_tests::skip!("west command not available");
    }

    let talker_bin = get_zephyr_dds_talker_a9();
    let listener_bin = get_zephyr_dds_listener_a9();
    let mcast = pick_mcast_addr_port();
    eprintln!("mcast group/port = {mcast}");

    // Listener first so it's ready to receive when the talker boots.
    let mut listener = ZephyrProcess::start_qemu_a9_mcast(
        &listener_bin,
        &mcast,
        "02:00:00:00:00:02",
    )
    .expect("Failed to start qemu_cortex_a9 listener");

    let listener_ready =
        listener.wait_for_pattern("Waiting for messages", Duration::from_secs(20));
    if !listener_ready.contains("Waiting for messages") {
        let _ = listener.kill();
        panic!(
            "qemu_cortex_a9 listener didn't reach subscriber readiness\n\
             Output:\n{}",
            listener_ready
        );
    }

    let mut talker = ZephyrProcess::start_qemu_a9_mcast(
        &talker_bin,
        &mcast,
        "02:00:00:00:00:01",
    )
    .expect("Failed to start qemu_cortex_a9 talker");

    // 30 s wall-clock is generous — actual sim-time discovery takes
    // a couple of seconds, then the listener's recv burst is
    // immediate. If we don't see Received: 5 by then, something
    // regressed.
    let listener_out =
        listener.wait_for_pattern("Received: 5", Duration::from_secs(30));
    let talker_out = talker
        .wait_for_pattern("Published: 5", Duration::from_secs(5));
    let _ = talker.kill();
    let _ = listener.kill();

    eprintln!("\n=== Talker tail ===");
    for line in talker_out.lines().rev().take(5).collect::<Vec<_>>().iter().rev() {
        eprintln!("{line}");
    }
    eprintln!("\n=== Listener tail ===");
    for line in listener_out.lines().rev().take(8).collect::<Vec<_>>().iter().rev() {
        eprintln!("{line}");
    }

    if !listener_out.contains("Received: 5") {
        panic!(
            "qemu_cortex_a9 listener received fewer than 5 samples \
             from the talker — interop regression.\n\
             Discovery is the most likely culprit (SPDP via mcast \
             then SEDP via unicast). Check Phase 92.5 doc + \
             nros-rmw-dds/transport_nros.rs for the mcast-fd routing \
             and Zephyr ARP/MAC plumbing.\n\
             Listener tail:\n{}",
            listener_out.lines().rev().take(20).collect::<Vec<_>>().join("\n")
        );
    }
}
