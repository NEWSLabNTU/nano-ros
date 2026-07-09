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

use nros_tests::{
    count_pattern,
    fixtures::{
        XrceAgent, ZenohRouter, build_native_listener, build_native_service_client,
        build_native_service_server, build_native_talker, require_xrce_agent,
    },
    output, platform,
    zephyr::{
        ZephyrPlatform, ZephyrProcess, get_prebuilt_zephyr_example,
        get_prebuilt_zephyr_workspace_entry, is_zephyr_available, require_zephyr,
        zephyr_workspace_path,
    },
};
use std::{path::PathBuf, time::Duration};

fn count_zephyr_received(output: &str) -> usize {
    // All Zephyr listener fixtures (c/cpp/rust) print the canonical
    // `Received: <n>` (Phase 198.2 normalized the rust fixture off `Received[`).
    output
        .lines()
        .filter(|line| line.contains(nros_tests::output::LISTENER_LOG_PREFIX))
        .count()
}

/// Get prebuilt Zephyr talker for native_sim (uses existing binary if available)
fn get_zephyr_talker_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-rs-talker binary")
}

/// Get prebuilt Zephyr listener for native_sim (uses existing binary if available)
fn get_zephyr_listener_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-listener", ZephyrPlatform::NativeSim)
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

    // #166 / phase-286 W1 — per-test ephemeral zenohd + locator override. The
    // image reads `-testargs --nros-locator=<loc>` and dials THIS router instead
    // of its build-time-baked port, so this test no longer needs the fixed
    // per-(variant,lang) port and no longer has to serialize against its siblings.
    eprintln!("Starting per-test zenohd router (ephemeral port)...");
    let router = ZenohRouter::start_unique().expect("Failed to start zenohd");
    let locator = router.locator();
    eprintln!("zenohd started on {locator}");

    // Resolve prebuilt examples (to separate directories)
    let talker_binary = get_zephyr_talker_native_sim();
    let listener_binary = get_zephyr_listener_native_sim();

    eprintln!("Talker binary: {}", talker_binary.display());
    eprintln!("Listener binary: {}", listener_binary.display());

    // Start listener first (so it creates its subscriber before talker publishes)
    eprintln!("Starting Zephyr listener...");
    let listener =
        ZephyrProcess::start_with_locator(&listener_binary, ZephyrPlatform::NativeSim, &locator)
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
    let mut listener = listener;

    // Start talker
    eprintln!("Starting Zephyr talker...");
    let mut talker =
        ZephyrProcess::start_with_locator(&talker_binary, ZephyrPlatform::NativeSim, &locator)
            .expect("Failed to start Zephyr talker");

    // Wait for communication
    eprintln!("Waiting for Zephyr talker → listener communication...");

    // Probe for the talker's 3rd publish + the listener's 3rd
    // Received marker, early-exiting as soon as both have emitted
    // enough output. Under `max-threads = 3` parallel load the
    // native_sim cold-boot + session open can take >8 s, so the
    // old fixed-8 s `wait_for_output` regularly missed the first
    // couple of publishes. 30 s cap is comfortable headroom.
    let _ = talker.wait_for_pattern(
        nros_tests::output::talker_line(3).as_str(),
        Duration::from_secs(30),
    );
    let _ = listener.wait_for_pattern(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(30),
    );
    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Kill processes
    talker.kill();
    listener.kill();
    drop(router);

    eprintln!("\n=== Talker output ===\n{}", talker_output);
    eprintln!("\n=== Listener output ===\n{}", listener_output);

    // Check talker status
    let talker_published = output::parse_talker(&talker_output).published_count > 0;
    let talker_connected = !talker_output.contains("session error");
    let talker_created_pub = talker_output.contains("Declared publisher")
        || talker_output.contains("Publisher created")
        || talker_output.contains(nros_tests::output::TALKER_READY_MARKER);

    // Check listener status
    let listener_received = count_zephyr_received(&listener_output) > 0;
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
                count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX)
            );
            // Don't fail the test - this is a known limitation
            return;
        }
        panic!("Listener failed to create subscriber and talker didn't publish");
    }

    // Check for known zenoh-pico limitation: transport TX failure when multiple clients connect
    let talker_tx_failed = talker_output.contains("Failed to publish");

    if talker_published && listener_received {
        let count = count_zephyr_received(&listener_output);
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

    // #166 / phase-286 W1 — per-test ephemeral zenohd + locator override (both
    // the native listener via NROS_LOCATOR and the Zephyr talker via
    // `-testargs --nros-locator` dial THIS router), so this test no longer needs
    // the fixed per-(variant,lang) port and can run parallel with its siblings.
    eprintln!("Starting per-test zenohd router (ephemeral, #166)...");
    let router = ZenohRouter::start_unique().expect("Failed to start zenohd");
    let locator = router.locator();
    eprintln!("zenohd locator: {locator}");

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
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("native listener did not become ready");

    // Start Zephyr talker
    eprintln!("Starting Zephyr talker...");
    let mut zephyr =
        ZephyrProcess::start_with_locator(&zephyr_binary, ZephyrPlatform::NativeSim, &locator)
            .expect("Failed to start Zephyr talker");

    // Wait for communication
    eprintln!("Waiting for Zephyr → Native communication...");

    // Wait for listener output (use wait_for_all_output to capture stderr where env_logger logs).
    // 40 s: on a slow native_sim host the Zephyr talker's zenoh-pico session
    // setup + first publish lands ~20 s after boot (issue #17). The wait always
    // runs the full duration (listener never self-exits), so this caps
    // wall-time, not the success path.
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(40))
        .expect("Listener timed out");

    // Get Zephyr output for debugging
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Kill processes
    zephyr.kill();
    drop(listener);
    drop(router);

    eprintln!("\n=== Zephyr output ===\n{}", zephyr_output);
    eprintln!("\n=== Native listener output ===\n{}", listener_output);

    // Strict delivery check: the native listener must log at least one
    // real "Received: <N>" line (not setup text like "Waiting for Int32 ...").
    let received_count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
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

    // #166 / phase-286 W1 — per-test ephemeral zenohd + locator override.
    eprintln!("Starting per-test zenohd router (ephemeral, #166)...");
    let router = ZenohRouter::start_unique().expect("Failed to start zenohd");
    let locator = router.locator();
    eprintln!("zenohd locator: {locator}");

    // Build native talker
    let talker_path = build_native_talker().expect("Failed to build native-rs-talker");

    // Get Zephyr listener
    let zephyr_binary = get_zephyr_listener_native_sim();
    eprintln!("Zephyr listener binary: {}", zephyr_binary.display());

    // Start Zephyr listener first (so it subscribes before talker publishes)
    eprintln!("Starting Zephyr listener...");
    let mut zephyr =
        ZephyrProcess::start_with_locator(&zephyr_binary, ZephyrPlatform::NativeSim, &locator)
            .expect("Failed to start Zephyr listener");

    let _ = zephyr.wait_for_pattern("Waiting for messages", Duration::from_secs(30));

    // Start native talker connecting to zenohd
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut talker_cmd = Command::new(talker_path);
    // Both connect to zenohd on localhost
    talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut talker = ManagedProcess::spawn_command(talker_cmd, "native-rs-talker")
        .expect("Failed to start talker");

    // Wait for communication
    eprintln!("Waiting for Native → Zephyr communication...");

    // Wait for Zephyr output. 40 s: the Zephyr listener's zenoh-pico
    // subscription setup is slow on a slow native_sim host (issue #17); the
    // fast native talker only delivers once the subscriber is declared.
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(40))
        .unwrap_or_default();

    // Get native talker output for debugging
    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(1))
        .unwrap_or_default();

    // Kill processes
    zephyr.kill();
    drop(talker);
    drop(router);

    eprintln!("\n=== Native talker output ===\n{}", talker_output);
    eprintln!("\n=== Zephyr listener output ===\n{}", zephyr_output);

    // Strict delivery check: the Zephyr listener must log at least one
    // canonical `Received: <n>` sample line (all c/cpp/rust fixtures, 198.2).
    let received_count = count_zephyr_received(&zephyr_output);
    let zephyr_transport_err = zephyr_output.contains("Transport(ConnectionFailed)")
        || zephyr_output.contains("z_declare_subscriber failed")
        || zephyr_output.contains("Failed to create subscriber");
    let talker_published = talker_output.contains(nros_tests::output::TALKER_LOG_PREFIX);

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

    // #166 / phase-286 W1 — per-test ephemeral zenohd + locator override (all
    // four peers dial THIS router: natives via NROS_LOCATOR, Zephyr images via
    // `-testargs --nros-locator`).
    eprintln!("Starting per-test zenohd router (ephemeral, #166)...");
    let router = ZenohRouter::start_unique().expect("Failed to start zenohd");
    let locator = router.locator();
    eprintln!("zenohd locator: {locator}");

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
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut native_listener =
        ManagedProcess::spawn_command(native_listener_cmd, "native-rs-listener")
            .expect("Failed to start native listener");

    // Note: Running multiple Zephyr processes simultaneously can cause issues
    // due to TAP interface conflicts. For this test, we use a staggered approach.
    let mut zephyr_listener = ZephyrProcess::start_with_locator(
        &zephyr_listener_binary,
        ZephyrPlatform::NativeSim,
        &locator,
    )
    .expect("Failed to start Zephyr listener");

    native_listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("native listener did not become ready");
    let _ = zephyr_listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));

    // Start talkers
    eprintln!("Starting talkers...");

    let mut native_talker_cmd = Command::new(native_talker_path);
    native_talker_cmd
        .env("NROS_LOCATOR", &locator)
        .env("RUST_LOG", "info");
    let mut native_talker = ManagedProcess::spawn_command(native_talker_cmd, "native-rs-talker")
        .expect("Failed to start native talker");

    let mut zephyr_talker = ZephyrProcess::start_with_locator(
        &zephyr_talker_binary,
        ZephyrPlatform::NativeSim,
        &locator,
    )
    .expect("Failed to start Zephyr talker");

    eprintln!("Waiting for bidirectional communication...");
    // 45 s: both directions gate on a slow native_sim Zephyr endpoint (issue
    // #17) — the native listener waits on the slow Zephyr talker's first
    // publish (~20 s after boot), and the Zephyr listener's own subscription
    // setup is slow before the fast native talker's samples land.
    let native_ready_output = native_listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            1,
            Duration::from_secs(45),
        )
        .unwrap_or_default();
    let _ = zephyr_listener.wait_for_pattern(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(45),
    );

    // Collect outputs
    let native_remaining = native_listener
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let native_listener_output = format!("{native_ready_output}{native_remaining}");
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
    zephyr_talker.kill();
    zephyr_listener.kill();
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

    // Strict delivery counts: match only real sample lines, not setup
    // text like "Waiting for Int32 messages ...". All fixtures log the
    // canonical `Received: <n>` (198.2).
    let native_received_count = count_pattern(
        &native_listener_output,
        nros_tests::output::LISTENER_LOG_PREFIX,
    );
    let zephyr_received_count = count_zephyr_received(&zephyr_listener_output);

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

// (Phase 182.3) `test_zephyr_{talker,listener}_build` removed — build-only
// fixture-presence checks, covered by `just zephyr build-fixtures` (the west
// prebuild that test-all depends on) + the zephyr pub/sub e2e tests that
// build+run the same prebuilt binaries.

// =============================================================================
// Zephyr Action Examples
// =============================================================================

/// Get prebuilt Zephyr action server for native_sim
fn get_zephyr_action_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-action-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-rs-action-server binary")
}

/// Get prebuilt Zephyr action client for native_sim
fn get_zephyr_action_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-action-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-rs-action-client binary")
}

// (Phase 182.3) `test_zephyr_action_{server,client}_build` removed — build-only
// presence checks (see the note above). The action e2e tests below build+run
// the same prebuilt binaries via `get_zephyr_action_{server,client}_native_sim`.

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
    let router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust),
    )
    .expect("Failed to start zenohd");
    eprintln!(
        "zenohd started on port {}",
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Rust)
    );

    // Resolve prebuilt examples
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
    //
    // Phase 160.C — bumped 30s → 60s. Each of the 3 service-server
    // (queryable) declarations on Zephyr serializes at ~10 s under the
    // current zenoh-pico transport (every declare does a sync wait that
    // hits an internal lease/keepalive boundary; native_sim NSOS does
    // not exhibit it on POSIX). 3 × 10 s + headroom puts readiness at
    // ~30 s, right on the prior cutoff. Root-cause investigation
    // (zenoh-pico declare-flush behaviour under Z_FEATURE_INTEREST=1)
    // remains open as a follow-up.
    // M-F.23: the single-node `zephyr_component_main!` macro emits the
    // canonical "Waiting for messages" readiness marker for every node
    // (pub/sub/service/action), so key readiness off that.
    let server_ready = server.wait_for_pattern("Waiting for messages", Duration::from_secs(60));
    if !server_ready.contains("Waiting for messages") {
        panic!(
            "Zephyr action server didn't reach readiness within 60 s.\nOutput:\n{}",
            server_ready
        );
    }
    let mut server = server;

    // Start action client
    eprintln!("Starting Zephyr action client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr action client");

    // Wait for action communication
    eprintln!("Waiting for action communication...");

    // Wait for the client to print the action-completion marker
    // (early-exits as soon as the action completes; falls back to a
    // long cap so a stuck client still returns and surfaces the
    // failure). Phase 160.C.2 — bumped 40 s → 150 s. Client
    // budget: ~22 s setup (3 service-client cascades on Zephyr
    // zenoh-pico, ~7 s each despite the upstream BATCH_UNICAST_SIZE
    // bump — still slower than POSIX) + 5 s send_goal + ~25 s
    // feedback stream (10 increments × 2.5 s) + 30 s get_result
    // (with `NROS_SERVICE_TIMEOUT_MS` raised to 30 s). Total
    // ~110 s. 150 s leaves headroom for `max-threads = 3`
    // parallelism load.
    let client_output = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(150),
    );
    // Server output can stop shortly after the client finishes —
    // give the reader a few seconds to drain any trailing feedback.
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    // Kill processes
    server.kill();
    client.kill();
    drop(router);

    eprintln!("\n=== Action Server output ===\n{}", server_output);
    eprintln!("\n=== Action Client output ===\n{}", client_output);

    // Check server status
    let server_connected =
        server_output.contains("Session opened") || server_output.contains("Waiting for messages");
    let server_created_queryables =
        server_output.contains("Queryable") || server_output.contains("ready");
    let server_received_goal = server_output.contains("Received goal")
        || server_output.contains("Goal accepted")
        || server_output.contains("Goal request");

    // Check client status
    let client_connected =
        client_output.contains("Session opened") || client_output.contains("Waiting for messages");
    let _client_subscribed = client_output.contains("Feedback subscriber ready")
        || client_output.contains("Subscriber created");
    let client_got_feedback = client_output.contains(nros_tests::output::ACTION_FEEDBACK_PREFIX)
        || client_output.contains("feedback");
    let client_completed = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);

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
        let feedback_count =
            count_pattern(&client_output, nros_tests::output::ACTION_FEEDBACK_PREFIX);
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

/// Get prebuilt Zephyr service server for native_sim
fn get_zephyr_service_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-service-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-rs-service-server binary")
}

/// Get prebuilt Zephyr service client for native_sim
fn get_zephyr_service_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-rs-service-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-rs-service-client binary")
}

// (Phase 182.3) `test_zephyr_service_{server,client}_build` removed — build-only
// presence checks (see the note above the action examples). The service smoke /
// e2e tests below build+run the same prebuilt binaries.

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
    let router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust),
    )
    .expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

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
                platform::ZEPHYR
                    .zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust)
            ),
        )
        .env("RUST_LOG", "info");
    let mut server = ManagedProcess::spawn_command(server_cmd, "native-rs-service-server")
        .expect("Failed to start native service server");

    server
        .wait_for_output_pattern("Waiting for service", Duration::from_secs(5))
        .expect("native service server did not become ready");

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
    zephyr.kill();
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
    let zephyr_got_response = zephyr_output.contains(nros_tests::output::SERVICE_RESULT_PREFIX);

    // Check native server status
    let server_received = server_output
        .contains(nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER)
        || server_output.contains("Received request")
        || server_output.contains("Request:");

    if zephyr_got_response {
        let response_count =
            count_pattern(&zephyr_output, nros_tests::output::SERVICE_RESULT_PREFIX);
        eprintln!(
            "\nSUCCESS: Zephyr client received {} responses from native server",
            response_count
        );
    } else if zephyr_connected && zephyr_sent_request {
        panic!(
            "Zephyr service E2E failed — client sent requests but all timed out.\n\
             Server received request: {}\n\
             This indicates a zenoh queryable discovery issue. Verify:\n\
             - Zephyr binary rebuilt after CMakeLists.txt changes: `just zephyr build-fixtures`\n\
             - zenohd running on bridge IP and reachable from both native and Zephyr processes",
            server_received
        );
    } else if !zephyr_connected {
        panic!(
            "Zephyr service E2E failed — client did not connect to zenohd.\n\
             Verify:\n\
             - Zephyr binary up to date: run `just zephyr build-fixtures`\n\
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

/// Get prebuilt Zephyr XRCE Rust talker for native_sim
fn get_zephyr_xrce_rs_talker_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-rs-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-rs-talker binary")
}

/// Get prebuilt Zephyr XRCE Rust listener for native_sim
fn get_zephyr_xrce_rs_listener_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-rs-listener", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-rs-listener binary")
}

/// Get prebuilt Zephyr XRCE C talker for native_sim
fn get_zephyr_xrce_c_talker_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-c-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-c-talker binary")
}

/// Get prebuilt Zephyr XRCE C listener for native_sim
fn get_zephyr_xrce_c_listener_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-c-listener", ZephyrPlatform::NativeSim)
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
/// - XRCE Agent available: `just zephyr setup` or `just xrce setup`
#[test]
fn test_zephyr_xrce_rust_talker_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    // Per-(variant, lang) Agent port — fixtures rebuilt with this same
    // port via the matching `xrce_port` column in just/zephyr.just.
    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust);
    eprintln!("Starting XRCE Agent on port {}...", port);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");

    // Resolve prebuilt examples
    let talker_binary = get_zephyr_xrce_rs_talker_native_sim();
    let listener_binary = get_zephyr_xrce_rs_listener_native_sim();

    eprintln!("Talker binary: {}", talker_binary.display());
    eprintln!("Listener binary: {}", listener_binary.display());

    // Start listener first (subscribe before publish)
    eprintln!("Starting Zephyr XRCE listener...");
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE listener");

    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));

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
    talker.kill();
    listener.kill();

    eprintln!("\n=== XRCE Talker output ===\n{}", talker_output);
    eprintln!("\n=== XRCE Listener output ===\n{}", listener_output);

    // Check talker status
    let talker_published = output::parse_talker(&talker_output).published_count > 0;
    let talker_error = talker_output.contains("Error:");

    // Check listener status
    let listener_received = count_zephyr_received(&listener_output) > 0;
    let listener_waiting = listener_output.contains("Waiting for messages");
    let listener_error = listener_output.contains("Error:");

    if talker_error {
        panic!("XRCE talker encountered an error:\n{}", talker_output);
    }
    if listener_error && !listener_received {
        panic!("XRCE listener encountered an error:\n{}", listener_output);
    }

    if listener_received {
        let count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
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
/// - XRCE Agent available: `just zephyr setup` or `just xrce setup`
#[test]
// Previously #[ignore]: C talker didn't flush XRCE output stream after publish (fixed)
fn test_zephyr_xrce_c_talker_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let port =
        platform::ZEPHYR.xrce_agent_port_for(platform::TestVariant::Pubsub, platform::TestLang::C);
    eprintln!("Starting XRCE Agent on port {}...", port);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");

    // Resolve prebuilt examples
    let talker_binary = get_zephyr_xrce_c_talker_native_sim();
    let listener_binary = get_zephyr_xrce_c_listener_native_sim();

    eprintln!("C Talker binary: {}", talker_binary.display());
    eprintln!("C Listener binary: {}", listener_binary.display());

    // Start listener first (subscribe before publish)
    eprintln!("Starting Zephyr XRCE C listener...");
    let mut listener = ZephyrProcess::start(&listener_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE C listener");

    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));

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
    talker.kill();
    listener.kill();

    eprintln!("\n=== XRCE C Talker output ===\n{}", talker_output);
    eprintln!("\n=== XRCE C Listener output ===\n{}", listener_output);

    // Check talker status (C API uses LOG_INF format)
    let talker_published = output::parse_talker(&talker_output).published_count > 0;
    let talker_init = talker_output.contains(nros_tests::output::TALKER_READY_MARKER);
    let talker_error =
        talker_output.contains("failed") || talker_output.contains("Network not ready");

    // Check listener status (C API uses LOG_INF format)
    let listener_received = listener_output.contains(nros_tests::output::LISTENER_LOG_PREFIX);
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
        let count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
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
    get_prebuilt_zephyr_example("zephyr-xrce-rs-service-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-rs-service-server binary")
}

fn get_zephyr_xrce_rs_service_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-rs-service-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-rs-service-client binary")
}

fn get_zephyr_xrce_rs_action_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-rs-action-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-rs-action-server binary")
}

fn get_zephyr_xrce_rs_action_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-rs-action-client", ZephyrPlatform::NativeSim)
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

    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Service, platform::TestLang::Rust);
    eprintln!("Starting XRCE Agent on port {}...", port);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_binary = get_zephyr_xrce_rs_service_server_native_sim();
    let client_binary = get_zephyr_xrce_rs_service_client_native_sim();

    eprintln!("XRCE service server binary: {}", server_binary.display());
    eprintln!("XRCE service client binary: {}", client_binary.display());

    eprintln!("Starting Zephyr XRCE service server...");
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE service server");
    let _ = server.wait_for_pattern(
        nros_tests::output::SERVICE_SERVER_READY_MARKER,
        Duration::from_secs(30),
    );

    eprintln!("Starting Zephyr XRCE service client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE service client");

    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();

    client.kill();
    server.kill();

    eprintln!("\n=== XRCE service server output ===\n{}", server_output);
    eprintln!("\n=== XRCE service client output ===\n{}", client_output);

    let response_count = count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
    let request_count = count_pattern(
        &server_output,
        nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
    );

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
/// 3. Verifies the client's terminal `Result received: [...]` line
#[test]
fn test_zephyr_xrce_rust_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Action, platform::TestLang::Rust);
    eprintln!("Starting XRCE Agent on port {}...", port);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_binary = get_zephyr_xrce_rs_action_server_native_sim();
    let client_binary = get_zephyr_xrce_rs_action_client_native_sim();

    eprintln!("XRCE action server binary: {}", server_binary.display());
    eprintln!("XRCE action client binary: {}", client_binary.display());

    eprintln!("Starting Zephyr XRCE action server...");
    let server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE action server");

    let server_ready = server.wait_for_pattern(
        nros_tests::output::ACTION_SERVER_READY_MARKER,
        Duration::from_secs(30),
    );
    if !server_ready.contains(nros_tests::output::ACTION_SERVER_READY_MARKER) {
        panic!(
            "Zephyr XRCE action server didn't reach readiness within 30s.\nOutput:\n{}",
            server_ready
        );
    }
    // Was 500 ms — bumped to 1500 ms so the XRCE Agent has time to
    // propagate the server's CREATE_REPLIER ack back to the client
    // session under `just test-all` load (8 sibling Zephyr XRCE
    // workers + parallel rebuilds racing for the same Agent).
    std::thread::sleep(Duration::from_millis(1500));
    let mut server = server;

    eprintln!("Starting Zephyr XRCE action client...");
    let mut client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr XRCE action client");

    let client_output = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(60),
    );
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("\n=== XRCE action server output ===\n{}", server_output);
    eprintln!("\n=== XRCE action client output ===\n{}", client_output);

    let server_received_goal = server_output.contains("Received goal request")
        || server_output.contains(nros_tests::output::ACTION_EXECUTING_MARKER);
    let client_got_feedback = client_output.contains(nros_tests::output::ACTION_FEEDBACK_PREFIX);
    let client_completed = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);

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
// Phase 95.C — Zephyr XRCE C++ talker/listener/svc/action
// =============================================================================
//
// Six new example crates ported from cpp/zenoh to cpp/xrce. They
// build clean and the boot smoke tests below verify each one reaches
// readiness on native_sim/native/64. Dual-instance E2E tests are
// #[ignore]d pending follow-up — see comments at each ignored test.

// Boot smoke tests match the banner the example prints _before_
// `nros::init()`. xrce examples block in `nros::init()` without an
// agent on port 2018 — we don't run one here because that's the
// E2E setup. The banner proves the binary linked + booted clean.

#[test]
fn test_zephyr_xrce_cpp_talker_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_zephyr_xrce_cpp_talker_native_sim();
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce talker");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/xrce talker didn't boot:\n{}", out);
    }
}

#[test]
fn test_zephyr_xrce_cpp_listener_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_zephyr_xrce_cpp_listener_native_sim();
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce listener");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/xrce listener didn't boot:\n{}", out);
    }
}

// =============================================================================
// Phase 95.D — Zephyr DDS C++ talker/listener/svc/action boot tests
// =============================================================================

#[test]
fn test_zephyr_dds_cpp_talker_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-cpp-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-cpp-talker binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds talker");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/dds talker didn't boot:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_cpp_listener_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-cpp-listener", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-cpp-listener binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds listener");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/dds listener didn't boot:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_cpp_service_server_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin =
        get_prebuilt_zephyr_example("zephyr-dds-cpp-service-server", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-cpp-service-server binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds service server");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/dds service server didn't boot:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_cpp_service_client_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin =
        get_prebuilt_zephyr_example("zephyr-dds-cpp-service-client", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-cpp-service-client binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds service client");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("cpp/dds service client didn't boot:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_cpp_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let server_bin =
        get_prebuilt_zephyr_example("zephyr-dds-cpp-action-server", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-cpp-action-server binary");
    let client_bin =
        get_prebuilt_zephyr_example("zephyr-dds-cpp-action-client", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-cpp-action-client binary");

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds action server");
    let server_ready = server.wait_for_pattern(
        nros_tests::output::ACTION_SERVER_READY_MARKER,
        Duration::from_secs(30),
    );
    if !server_ready.contains(nros_tests::output::ACTION_SERVER_READY_MARKER) {
        panic!(
            "Zephyr C++ Cyclone action server didn't reach readiness.\nOutput:\n{}",
            server_ready
        );
    }

    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/dds action client");
    let client_output = client.wait_for_pattern("Result received", Duration::from_secs(60));
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!(
        "\n=== Zephyr C++ Cyclone action server output ===\n{}",
        server_output
    );
    eprintln!(
        "\n=== Zephyr C++ Cyclone action client output ===\n{}",
        client_output
    );

    let server_completed = server_output.contains(nros_tests::output::ACTION_GOAL_SUCCEEDED_MARKER);
    let client_completed = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);
    if !(server_completed && client_completed) {
        panic!(
            "C++ Cyclone action E2E failed (server_completed={}, client_completed={}).",
            server_completed, client_completed
        );
    }
}

#[test]
fn test_zephyr_dds_rs_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let server_bin =
        get_prebuilt_zephyr_example("zephyr-dds-rs-action-server", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-rs-action-server binary");
    let client_bin =
        get_prebuilt_zephyr_example("zephyr-dds-rs-action-client", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-rs-action-client binary");

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start rs/dds action server");
    // M-F.23: `zephyr_component_main!` emits "Waiting for messages" as the
    // universal readiness marker.
    let server_ready = server.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    if !server_ready.contains("Waiting for messages") {
        panic!(
            "Zephyr Rust Cyclone action server didn't reach readiness.\nOutput:\n{}",
            server_ready
        );
    }

    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start rs/dds action client");
    let client_output = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(90),
    );
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!(
        "\n=== Zephyr Rust Cyclone action server output ===\n{}",
        server_output
    );
    eprintln!(
        "\n=== Zephyr Rust Cyclone action client output ===\n{}",
        client_output
    );

    let server_received_goal = server_output.contains("Received goal request")
        || server_output.contains(nros_tests::output::ACTION_EXECUTING_MARKER);
    let client_completed = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);
    if !(server_received_goal && client_completed) {
        panic!(
            "Rust Cyclone action E2E failed (server_received_goal={}, client_completed={}).",
            server_received_goal, client_completed
        );
    }
}

// =============================================================================
// Phase 95.E — Zephyr DDS C talker/listener/svc/action boot tests
// =============================================================================

#[test]
fn test_zephyr_dds_c_talker_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-c-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-c-talker binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds talker");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("c/dds talker didn't print Zephyr banner:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_c_listener_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-c-listener", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-c-listener binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds listener");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("c/dds listener didn't print Zephyr banner:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_c_service_server_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-c-service-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-c-service-server binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds service server");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("c/dds service server didn't print Zephyr banner:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_c_service_client_boots() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let bin = get_prebuilt_zephyr_example("zephyr-dds-c-service-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-dds-c-service-client binary");
    let mut p = ZephyrProcess::start(&bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds service client");
    let out = p.wait_for_pattern("Booting Zephyr OS", Duration::from_secs(10));
    p.kill();
    if !out.contains("Booting Zephyr OS") {
        panic!("c/dds service client didn't print Zephyr banner:\n{}", out);
    }
}

#[test]
fn test_zephyr_dds_c_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    let server_bin =
        get_prebuilt_zephyr_example("zephyr-dds-c-action-server", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-c-action-server binary");
    let client_bin =
        get_prebuilt_zephyr_example("zephyr-dds-c-action-client", ZephyrPlatform::NativeSim)
            .expect("Failed to get zephyr-dds-c-action-client binary");

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds action server");
    let server_ready = server.wait_for_pattern("Waiting for goals", Duration::from_secs(30));
    if !server_ready.contains("Waiting for goals") {
        panic!(
            "Zephyr C Cyclone action server didn't reach readiness.\nOutput:\n{}",
            server_ready
        );
    }

    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start c/dds action client");
    let client_output = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(60),
    );
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!(
        "\n=== Zephyr C Cyclone action server output ===\n{}",
        server_output
    );
    eprintln!(
        "\n=== Zephyr C Cyclone action client output ===\n{}",
        client_output
    );

    let server_completed = server_output.contains(nros_tests::output::ACTION_GOAL_SUCCEEDED_MARKER);
    let client_completed = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);
    if !(server_completed && client_completed) {
        panic!(
            "C Cyclone action E2E failed (server_completed={}, client_completed={}).",
            server_completed, client_completed
        );
    }
}

// =============================================================================
// Phase 95.C — Zephyr XRCE C++ E2E tests (dual-instance, #[ignore]d)
// =============================================================================

fn get_zephyr_xrce_cpp_talker_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-talker binary")
}

fn get_zephyr_xrce_cpp_listener_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-listener", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-listener binary")
}

fn get_zephyr_xrce_cpp_service_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-service-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-service-server binary")
}

fn get_zephyr_xrce_cpp_service_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-service-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-service-client binary")
}

fn get_zephyr_xrce_cpp_action_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-action-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-action-server binary")
}

fn get_zephyr_xrce_cpp_action_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("zephyr-xrce-cpp-action-client", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-xrce-cpp-action-client binary")
}

/// Phase 96.1 — cpp/xrce talker→listener interop on a shared agent.
///
/// Re-enabled after the cpp `nros::init()` overload took an explicit
/// session_name. Earlier the wrapper hardcoded the XRCE session key
/// to a hash of `"nros_cpp"` for every cpp process — two cpp
/// participants on the same agent collided as one client, so topic
/// publishes weren't cross-routed. Examples now pass distinct names
/// (`"zephyr_cpp_talker"`, `"zephyr_cpp_listener"`, …).
#[test]
fn test_zephyr_xrce_cpp_talker_listener() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let talker_bin = get_zephyr_xrce_cpp_talker_native_sim();
    let listener_bin = get_zephyr_xrce_cpp_listener_native_sim();

    let mut listener = ZephyrProcess::start(&listener_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr cpp/xrce listener");
    let _ = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));

    let mut talker = ZephyrProcess::start(&talker_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr cpp/xrce talker");

    let talker_output = talker
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(10))
        .unwrap_or_default();
    talker.kill();
    listener.kill();

    eprintln!("=== cpp/xrce talker output ===\n{}", talker_output);
    eprintln!("=== cpp/xrce listener output ===\n{}", listener_output);

    let listener_received = count_zephyr_received(&listener_output) > 0;
    if !listener_received {
        panic!(
            "cpp/xrce listener didn't receive any messages.\nTalker:\n{}\nListener:\n{}",
            talker_output, listener_output
        );
    }
    let count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    eprintln!("SUCCESS: cpp/xrce listener got {} messages", count);
}

/// Phase 96.1 — cpp/xrce service request/reply on a shared agent.
/// Re-enabled alongside `test_zephyr_xrce_cpp_talker_listener`.
#[test]
fn test_zephyr_xrce_cpp_service_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Service, platform::TestLang::Cpp);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_bin = get_zephyr_xrce_cpp_service_server_native_sim();
    let client_bin = get_zephyr_xrce_cpp_service_client_native_sim();

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce service server");
    let _ = server.wait_for_pattern("Waiting for service", Duration::from_secs(30));

    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce service client");

    let client_output = client
        .wait_for_output(Duration::from_secs(30))
        .unwrap_or_default();
    let server_output = server
        .wait_for_output(Duration::from_secs(3))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!("=== cpp/xrce service server output ===\n{}", server_output);
    eprintln!("=== cpp/xrce service client output ===\n{}", client_output);

    let ok_count = count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
    let request_count = count_pattern(
        &server_output,
        nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
    );
    if ok_count >= 1 {
        eprintln!(
            "SUCCESS: cpp/xrce service got {} responses, {} requests handled",
            ok_count, request_count
        );
    } else {
        panic!(
            "cpp/xrce service E2E failed (client OK={}, server requests={}).\nClient:\n{}\nServer:\n{}",
            ok_count, request_count, client_output, server_output
        );
    }
}

/// Phase 96.1 follow-up — cpp/xrce action goal/feedback/result on
/// a shared agent. Re-enabled after fixing two off-by-N offset
/// bugs in `arena.rs`'s action-client trampoline:
///   * result reply: `result_offset = 5` missed the 3-byte align
///     pad inserted by `try_handle_get_result_raw` between the
///     status byte and the payload (correct offset = 8).
///   * feedback: `offset = 4 + 16` missed the 4-byte GoalId
///     length-prefix u32 written by `write_goal_id` (correct
///     offset = 24).
///
/// Both surfaced as empty payloads on the cpp client side because
/// the prefix bytes leaked into the body and `ffi_deserialize`
/// read sequence_length = 0.
#[test]
fn test_zephyr_xrce_cpp_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }

    let port = platform::ZEPHYR
        .xrce_agent_port_for(platform::TestVariant::Action, platform::TestLang::Cpp);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_bin = get_zephyr_xrce_cpp_action_server_native_sim();
    let client_bin = get_zephyr_xrce_cpp_action_client_native_sim();

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce action server");
    let _ = server.wait_for_pattern("Waiting for goal", Duration::from_secs(30));

    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim)
        .expect("Failed to start cpp/xrce action client");

    let client_output = client
        .wait_for_output(Duration::from_secs(60))
        .unwrap_or_default();
    let server_output = server
        .wait_for_output(Duration::from_secs(5))
        .unwrap_or_default();
    client.kill();
    server.kill();

    eprintln!("=== cpp/xrce action server output ===\n{}", server_output);
    eprintln!("=== cpp/xrce action client output ===\n{}", client_output);

    let feedback = count_pattern(&client_output, "Feedback");
    let completed = client_output.contains("completed")
        || client_output.contains("Result")
        || client_output.contains("succeeded");
    if feedback >= 1 && completed {
        eprintln!("SUCCESS: cpp/xrce action got {} feedback frames", feedback);
    } else {
        panic!(
            "cpp/xrce action E2E failed (feedback={}, completed={}).\nClient:\n{}\nServer:\n{}",
            feedback, completed, client_output, server_output
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
    let router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust),
    )
    .expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

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

    let _ = zephyr.wait_for_pattern(
        nros_tests::output::SERVICE_SERVER_READY_MARKER,
        Duration::from_secs(30),
    );

    // Start native service client
    use nros_tests::process::ManagedProcess;
    use std::process::Command;

    let mut client_cmd = Command::new(client_path);
    client_cmd
        .env(
            "NROS_LOCATOR",
            format!(
                "tcp/127.0.0.1:{}",
                platform::ZEPHYR
                    .zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust)
            ),
        )
        .env("RUST_LOG", "info");
    let mut client = ManagedProcess::spawn_command(client_cmd, "native-rs-service-client")
        .expect("Failed to start native service client");

    // Get outputs
    let client_output = client
        .wait_for_output_count(
            nros_tests::output::SERVICE_RESULT_PREFIX,
            1,
            Duration::from_secs(30),
        )
        .unwrap_or_default();
    let zephyr_output = zephyr
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    // Kill processes
    zephyr.kill();
    drop(client);
    drop(router);

    eprintln!("\n=== Zephyr server output ===\n{}", zephyr_output);
    eprintln!("\n=== Native client output ===\n{}", client_output);

    // Check Zephyr server status
    let zephyr_connected = zephyr_output.contains("Session opened");
    let zephyr_ready = zephyr_output.contains(nros_tests::output::SERVICE_SERVER_READY_MARKER);
    let zephyr_received =
        zephyr_output.contains(nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER);
    let zephyr_replied = zephyr_output.contains("a: ");

    // Check native client status
    let client_got_response = client_output.contains(nros_tests::output::SERVICE_RESULT_PREFIX);

    if client_got_response {
        let response_count =
            count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
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
            "Service communication failed:\n  zephyr_connected={}\n  zephyr_ready={}\n  zephyr_received={}\n  zephyr_replied={}\n  client_response={}",
            zephyr_connected, zephyr_ready, zephyr_received, zephyr_replied, client_got_response
        );
    }
}

// =============================================================================
// Zephyr C++ E2E Tests
// =============================================================================

/// Get prebuilt Zephyr C++ talker for native_sim
fn get_zephyr_cpp_talker_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-talker", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-cpp-talker binary")
}

/// Get prebuilt Zephyr C++ listener for native_sim
fn get_zephyr_cpp_listener_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-listener", ZephyrPlatform::NativeSim)
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
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp),
    )
    .expect("Failed to start zenohd");
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
    let mut listener = listener;

    // Start talker
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim).unwrap();

    // Probe for the 3rd publish + 3rd Received, early-exiting
    // instead of a fixed 8 s wait that couldn't keep up with
    // `max-threads = 3` parallel cold-boot variance.
    let _ = talker.wait_for_pattern(
        nros_tests::output::talker_line(3).as_str(),
        Duration::from_secs(30),
    );
    let _ = listener.wait_for_pattern(
        nros_tests::output::listener_line(3).as_str(),
        Duration::from_secs(30),
    );

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
    let talker_published = output::parse_talker(&talker_output).published_count > 0;
    // Check listener received messages
    let listener_received =
        count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);

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
    } else if !talker_output.contains("Booting Zephyr OS") {
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

    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp),
    )
    .expect("Failed to start zenohd");
    // Build native Rust listener
    let native_listener = match build_native_listener() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            nros_tests::skip!("could not build native listener: {}", e);
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
            platform::ZEPHYR
                .zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp)
        ),
    );
    listener_cmd.env("RUST_LOG", "info");
    let mut listener =
        nros_tests::fixtures::ManagedProcess::spawn_command(listener_cmd, "native-listener")
            .expect("Failed to start native listener");

    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("native listener did not become ready");

    // Start Zephyr C++ talker
    let mut talker = ZephyrProcess::start(&talker_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for 2 messages: this test asserts `received_count >= 2` below, so
    // waiting for only 1 returned as soon as the first arrived and captured a
    // single "Received:" line, failing deterministically. The Zephyr C++
    // talker publishes repeatedly (~every 2.5 s after a 5 s warm-up), so 2
    // messages arrive well within the 30 s budget.
    let listener_output = listener
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            2,
            Duration::from_secs(30),
        )
        .unwrap_or_default();
    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("\n=== Native listener output ===\n{}", listener_output);
    eprintln!("\n=== Zephyr C++ talker output ===\n{}", talker_output);

    let received_count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);

    if received_count >= 2 {
        eprintln!(
            "\nSUCCESS: Native listener received {} messages from Zephyr C++ talker",
            received_count
        );
    } else if output::parse_talker(&talker_output).published_count > 0 {
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

    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp),
    )
    .expect("Failed to start zenohd");
    // Build native Rust talker
    let native_talker = match build_native_talker() {
        Ok(p) => p.to_path_buf(),
        Err(e) => {
            nros_tests::skip!("could not build native talker: {}", e);
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
    let mut listener = listener;

    // Start native talker (connects to zenohd)
    let mut talker_cmd = std::process::Command::new(&native_talker);
    talker_cmd.env(
        "NROS_LOCATOR",
        format!(
            "tcp/127.0.0.1:{}",
            platform::ZEPHYR
                .zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Cpp)
        ),
    );
    talker_cmd.env("RUST_LOG", "info");
    let mut talker =
        nros_tests::fixtures::ManagedProcess::spawn_command(talker_cmd, "native-talker")
            .expect("Failed to start native talker");

    // Probe for the 3rd Received on the Zephyr side (early-exits
    // instead of the old 8 s+3 s blind sleep that couldn't keep
    // up with parallel-load variance).
    let _ = listener.wait_for_pattern(
        nros_tests::output::listener_line(3).as_str(),
        Duration::from_secs(30),
    );

    let talker_output = talker
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();
    let listener_output = listener
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();

    eprintln!("\n=== Native talker output ===\n{}", talker_output);
    eprintln!("\n=== Zephyr C++ listener output ===\n{}", listener_output);

    let received_count = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);

    if received_count >= 2 {
        eprintln!(
            "\nSUCCESS: Zephyr C++ listener received {} messages from native talker",
            received_count
        );
    } else if talker_output.contains(nros_tests::output::TALKER_LOG_PREFIX) {
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

/// Get prebuilt Zephyr C++ service server for native_sim
fn get_zephyr_cpp_service_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-service-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-cpp-service-server binary")
}

/// Get prebuilt Zephyr C++ service client for native_sim
fn get_zephyr_cpp_service_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-service-client", ZephyrPlatform::NativeSim)
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
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Cpp),
    )
    .expect("Failed to start zenohd");
    let server_binary = get_zephyr_cpp_service_server_native_sim();
    let client_binary = get_zephyr_cpp_service_client_native_sim();

    eprintln!("C++ Service Server binary: {}", server_binary.display());
    eprintln!("C++ Service Client binary: {}", client_binary.display());

    // Start server first
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim).unwrap();
    let _ = server.wait_for_pattern("Waiting for service", Duration::from_secs(30));

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

    let ok_count = count_pattern(&client_output, nros_tests::output::SERVICE_RESULT_PREFIX);
    let request_count = count_pattern(
        &server_output,
        nros_tests::output::SERVICE_INCOMING_REQUEST_MARKER,
    );

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

/// Get prebuilt Zephyr C++ action server for native_sim
fn get_zephyr_cpp_action_server_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-action-server", ZephyrPlatform::NativeSim)
        .expect("Failed to get zephyr-cpp-action-server binary")
}

/// Get prebuilt Zephyr C++ action client for native_sim
fn get_zephyr_cpp_action_client_native_sim() -> PathBuf {
    get_prebuilt_zephyr_example("cpp-action-client", ZephyrPlatform::NativeSim)
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
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::Cpp),
    )
    .expect("Failed to start zenohd");
    let server_binary = get_zephyr_cpp_action_server_native_sim();
    let client_binary = get_zephyr_cpp_action_client_native_sim();

    eprintln!("C++ Action Server binary: {}", server_binary.display());
    eprintln!("C++ Action Client binary: {}", client_binary.display());

    // Start action server first
    let mut server = ZephyrProcess::start(&server_binary, ZephyrPlatform::NativeSim).unwrap();

    // Phase 160.C — wait for server readiness rather than fixed 3 s sleep.
    // `create_action_server` declares 3 queryables on Zephyr; each
    // serializes at ~10 s under the current zenoh-pico transport
    // (see test_zephyr_action_e2e for the same observation). Total
    // readiness time ~30 s — fixed-3-s sleep used to race the client
    // ahead of the server's first queryable and the test failed.
    let server_ready = server.wait_for_pattern(
        nros_tests::output::ACTION_SERVER_READY_MARKER,
        Duration::from_secs(60),
    );
    if !server_ready.contains(nros_tests::output::ACTION_SERVER_READY_MARKER) {
        panic!(
            "Zephyr C++ action server didn't reach readiness within 60 s.\nOutput:\n{}",
            server_ready
        );
    }
    // Start action client
    let client = ZephyrProcess::start(&client_binary, ZephyrPlatform::NativeSim).unwrap();

    // Wait for client to complete. Phase 160.C — bumped 30 s → 90 s.
    // Client itself takes ~25 s to reach `send_goal` (3 service-clients
    // mirror the server's 3 queryables — each declaration serializes at
    // ~10 s on Zephyr zenoh-pico). Then goal exec + get_result. Was
    // racing the 30 s window before this bump.
    let client_output = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(90),
    );
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

    let client_ok = client_output.contains(nros_tests::output::ACTION_RESULT_PREFIX);
    let server_completed = server_output.contains(nros_tests::output::ACTION_GOAL_SUCCEEDED_MARKER);

    if client_ok && server_completed {
        eprintln!("\nSUCCESS: C++ action server completed goal, client received result");
    } else if server_completed {
        panic!("Server completed goal but client didn't get result");
    } else if server_output.contains("Received goal request") {
        panic!("Server received goal but didn't complete");
    } else {
        panic!(
            "C++ action test failed (client OK={}, server completed={})",
            client_ok, server_completed
        );
    }
}

// =============================================================================
// Zephyr C E2E (Phase 183.1) — close the zephyr/c hole.
//
// `examples/zephyr/c/` ships 6 zenoh + 6 xrce cases, but the only C runtime
// coverage was `test_zephyr_xrce_c_talker_listener` (xrce pubsub). These add
// the C zenoh pub/sub + service + action e2e and the C xrce service + action,
// mirroring the cpp/rust suites. Binaries resolve via the per-(lang,case,rmw)
// CMake/Corrosion prebuild (`build-c-<case>-<rmw>/zephyr/zephyr.exe`), so each
// skips cleanly when `just zephyr build-fixtures` hasn't produced the cell.
// Not `#[ignore]`d: zephyr C zenoh/xrce are expected to run (the cyclone C
// e2e already does) — a runtime failure here is a real bug to surface.
// =============================================================================

/// Resolve a prebuilt zephyr C example for `case` + `rmw`, or skip.
fn zephyr_c_example(case: &str, rmw: nros_tests::fixtures::Rmw) -> std::path::PathBuf {
    nros_tests::fixtures::build_zephyr_cmake_example_rmw("c", case, rmw).unwrap_or_else(|e| {
        nros_tests::skip!(
            "zephyr/c/{case} {rmw:?} not prebuilt (run `just zephyr build-fixtures`): {e:?}"
        )
    })
}

/// Zephyr C zenoh talker → listener pub/sub.
#[test]
fn test_zephyr_c_talker_to_listener_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::C),
    )
    .expect("Failed to start zenohd");
    let listener_bin = zephyr_c_example("listener", nros_tests::fixtures::Rmw::Zenoh);
    let talker_bin = zephyr_c_example("talker", nros_tests::fixtures::Rmw::Zenoh);

    let listener = ZephyrProcess::start(&listener_bin, ZephyrPlatform::NativeSim).unwrap();
    let ready = listener.wait_for_pattern("Waiting for messages", Duration::from_secs(30));
    if !ready.contains("Waiting for messages") {
        panic!("Zephyr C zenoh listener not ready in 30 s.\nOutput:\n{ready}");
    }
    let mut listener = listener;
    let mut talker = ZephyrProcess::start(&talker_bin, ZephyrPlatform::NativeSim).unwrap();

    let listener_out = listener.wait_for_pattern(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(30),
    );
    talker.kill();
    listener.kill();
    eprintln!("=== zephyr C zenoh listener ===\n{listener_out}");
    assert!(
        listener_out.contains(nros_tests::output::LISTENER_LOG_PREFIX),
        "zephyr C zenoh listener received no sample:\n{listener_out}"
    );
}

/// Zephyr C zenoh service server ↔ client.
#[test]
fn test_zephyr_c_service_server_to_client_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::C),
    )
    .expect("Failed to start zenohd");
    let server_bin = zephyr_c_example("service-server", nros_tests::fixtures::Rmw::Zenoh);
    let client_bin = zephyr_c_example("service-client", nros_tests::fixtures::Rmw::Zenoh);

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim).unwrap();
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim).unwrap();

    let client_out = client.wait_for_pattern(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(30),
    );
    client.kill();
    server.kill();
    eprintln!("=== zephyr C zenoh service client ===\n{client_out}");
    assert!(
        client_out.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "zephyr C zenoh service client got no reply:\n{client_out}"
    );
}

/// Zephyr C zenoh action server ↔ client.
///
/// `#[ignore]`d — runtime-verified to fail (re-verified 2026-05-26 with fresh
/// NSOS fixtures + latest code). **Not 177.30** (that was the NuttX *C++*
/// `fflush(stdout)` deadlock, now fixed; the zephyr C app uses Zephyr `LOG_INF`,
/// no libc stdout lock). The failure is on the **server side**: the C zenoh
/// action server boots and logs "Network ready (NSOS)" but **never reaches
/// "Waiting for goals"** — it hangs during `create_action_server` (the
/// zenoh-pico declare path for the goal/cancel/result/feedback/status entities),
/// so the client never even sends. The zephyr **cpp + rust** zenoh action
/// servers reach readiness and pass, so this is **C-specific** (the nros-c
/// action-server setup over zenoh-pico on zephyr). pub/sub + service C pass on
/// the same fixture. Distinct open gap — needs its own investigation.
#[test]
#[ignore = "zephyr C zenoh action server hangs in create_action_server (never reaches 'Waiting for goals'); C-specific, NOT 177.30; cpp/rust pass"]
fn test_zephyr_c_action_server_to_client_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Action, platform::TestLang::C),
    )
    .expect("Failed to start zenohd");
    let server_bin = zephyr_c_example("action-server", nros_tests::fixtures::Rmw::Zenoh);
    let client_bin = zephyr_c_example("action-client", nros_tests::fixtures::Rmw::Zenoh);

    let server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim).unwrap();
    let ready = server.wait_for_pattern("Waiting for goals", Duration::from_secs(30));
    if !ready.contains("Waiting for goals") {
        panic!("Zephyr C zenoh action server not ready in 30 s.\nOutput:\n{ready}");
    }
    let mut server = server;
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim).unwrap();

    let client_out = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(60),
    );
    client.kill();
    server.kill();
    eprintln!("=== zephyr C zenoh action client ===\n{client_out}");
    assert!(
        client_out.contains(nros_tests::output::ACTION_RESULT_PREFIX),
        "zephyr C zenoh action client did not complete:\n{client_out}"
    );
}

/// Zephyr C XRCE service server ↔ client.
#[test]
fn test_zephyr_xrce_c_service_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    let port =
        platform::ZEPHYR.xrce_agent_port_for(platform::TestVariant::Service, platform::TestLang::C);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_bin = zephyr_c_example("service-server", nros_tests::fixtures::Rmw::Xrce);
    let client_bin = zephyr_c_example("service-client", nros_tests::fixtures::Rmw::Xrce);

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim).unwrap();
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim).unwrap();

    let client_out = client.wait_for_pattern(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(30),
    );
    client.kill();
    server.kill();
    eprintln!("=== zephyr C xrce service client ===\n{client_out}");
    assert!(
        client_out.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "zephyr C xrce service client got no reply:\n{client_out}"
    );
}

/// Zephyr C XRCE action server ↔ client.
#[test]
fn test_zephyr_xrce_c_action_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    if !require_xrce_agent() {
        nros_tests::skip!("XRCE agent not available");
    }
    let port =
        platform::ZEPHYR.xrce_agent_port_for(platform::TestVariant::Action, platform::TestLang::C);
    let _agent = XrceAgent::start(port).expect("Failed to start XRCE Agent");
    let server_bin = zephyr_c_example("action-server", nros_tests::fixtures::Rmw::Xrce);
    let client_bin = zephyr_c_example("action-client", nros_tests::fixtures::Rmw::Xrce);

    let server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim).unwrap();
    let ready = server.wait_for_pattern("Waiting for goals", Duration::from_secs(30));
    if !ready.contains("Waiting for goals") {
        panic!("Zephyr C xrce action server not ready in 30 s.\nOutput:\n{ready}");
    }
    let mut server = server;
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim).unwrap();

    let client_out = client.wait_for_pattern(
        nros_tests::output::ACTION_RESULT_PREFIX,
        Duration::from_secs(60),
    );
    client.kill();
    server.kill();
    eprintln!("=== zephyr C xrce action client ===\n{client_out}");
    assert!(
        client_out.contains(nros_tests::output::ACTION_RESULT_PREFIX),
        "zephyr C xrce action client did not complete:\n{client_out}"
    );
}

// =============================================================================
// Zephyr Rust zenoh service E2E (Phase 183.3) — rust had pubsub + action e2e
// but no service (the cpp sibling did). Reuses the existing
// `get_zephyr_service_{server,client}_native_sim` (rust zenoh) resolvers.
// =============================================================================

/// Zephyr Rust zenoh service server ↔ client.
#[test]
fn test_zephyr_rust_service_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }
    let _router = ZenohRouter::start(
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Service, platform::TestLang::Rust),
    )
    .expect("Failed to start zenohd");
    let server_bin = get_zephyr_service_server_native_sim();
    let client_bin = get_zephyr_service_client_native_sim();

    let mut server = ZephyrProcess::start(&server_bin, ZephyrPlatform::NativeSim).unwrap();
    let _ = server.wait_for_pattern("Waiting", Duration::from_secs(30));
    let mut client = ZephyrProcess::start(&client_bin, ZephyrPlatform::NativeSim).unwrap();

    let client_out = client.wait_for_pattern(
        nros_tests::output::SERVICE_RESULT_PREFIX,
        Duration::from_secs(30),
    );
    client.kill();
    server.kill();
    eprintln!("=== zephyr rust zenoh service client ===\n{client_out}");
    assert!(
        client_out.contains(nros_tests::output::SERVICE_RESULT_PREFIX),
        "zephyr rust zenoh service client got no reply:\n{client_out}"
    );
}

// =============================================================================
// Zephyr workspace Entry E2E (Phase 225.P)
//
// The workspace Entry (`examples/workspaces/rust/src/zephyr_entry`) is the
// Zephyr sibling of the native / FreeRTOS / ThreadX workspace Entries: a
// SINGLE Zephyr application that hosts the whole launch-defined node set —
// talker AND listener — in one process via
// `nros::main!(launch = "demo_bringup:system.launch.xml")`. Built by the
// 225.P west lane into `build-ws-rs-entry-zenoh` and resolved here through
// `get_prebuilt_zephyr_workspace_entry()`.
//
// Single-session caveat: zenoh does NOT loop a session's own publications
// back to a subscriber in that same session, so the Entry's in-process
// listener cannot observe the in-process talker. We therefore assert
// delivery to a SECOND, EXTERNAL native listener — the same shape as the
// single-node Zephyr rust pubsub E2E — which is a real cross-process
// pub/sub observation through generated `std_msgs/Int32` on `/chatter`.
//
// The Entry's baked locator + the external listener's `NROS_LOCATOR` + the
// zenohd router all use the Zephyr rust-pubsub port (7456). The Entry
// therefore shares that port with the single-node rust pubsub talker and
// must serialize with it — this test is routed into the
// `qemu-zephyr-pubsub-rust` nextest group.
// =============================================================================

/// Zephyr workspace Entry boots on native_sim, brings up its launch node set
/// (talker + listener in one process), and its `/chatter` publications are
/// delivered cross-process to an external native listener.
#[test]
fn test_zephyr_workspace_entry_native_sim_e2e() {
    if !require_zephyr() {
        nros_tests::skip!("Zephyr not available");
    }

    // Resolve the prebuilt workspace-Entry binary. Tests never build
    // fixtures in-body; a missing/stale image fails fast with a
    // `just zephyr build-fixtures` hint.
    let entry_binary = get_prebuilt_zephyr_workspace_entry().expect(
        "Failed to resolve prebuilt Zephyr workspace Entry — \
         run `just zephyr build-fixtures` first",
    );
    eprintln!("Workspace Entry binary: {}", entry_binary.display());

    // Start zenohd on the Zephyr rust-pubsub port — the Entry's baked
    // locator points here, and so does the external listener below.
    let port =
        platform::ZEPHYR.zenohd_port_for(platform::TestVariant::Pubsub, platform::TestLang::Rust);
    eprintln!("Starting zenohd router on port {port}...");
    let router = ZenohRouter::start(port).expect("Failed to start zenohd");
    eprintln!("zenohd locator: {}", router.locator());

    // Build + start an EXTERNAL native listener on the same locator. The
    // Entry's talker publishes `/chatter`; this listener is the observable
    // delivery endpoint (the Entry's own in-process listener sees nothing —
    // no same-session zenoh loopback).
    let listener_path = build_native_listener().expect("Failed to build native-rs-listener");
    use nros_tests::process::ManagedProcess;
    use std::process::Command;
    let mut listener_cmd = Command::new(listener_path);
    listener_cmd
        .env("NROS_LOCATOR", format!("tcp/127.0.0.1:{port}"))
        .env("RUST_LOG", "info");
    let mut listener = ManagedProcess::spawn_command(listener_cmd, "native-rs-listener")
        .expect("Failed to start listener");
    listener
        .wait_for_output_pattern("Waiting for", Duration::from_secs(5))
        .expect("native listener did not become ready");

    // Boot the single-process Entry (talker + listener). `ZephyrProcess::Drop`
    // kills it, so no manual teardown is required on an early panic.
    eprintln!("Starting Zephyr workspace Entry...");
    let mut entry = ZephyrProcess::start(&entry_binary, ZephyrPlatform::NativeSim)
        .expect("Failed to start Zephyr workspace Entry");

    // The external listener must log at least one real `Received:` line.
    // Timeout is generous: on a slow native_sim host the Entry's zenoh-pico
    // session setup + first publish lands ~20 s after boot (steady-state
    // cadence then tracks the ~2.5 s lease keepalive). `wait_for_all_output`
    // always runs the full duration (the listener `spin_blocking`s and never
    // self-exits), so this bounds the test wall-time, not its success path.
    let listener_output = listener
        .wait_for_all_output(Duration::from_secs(40))
        .expect("Listener timed out");
    let entry_output = entry
        .wait_for_output(Duration::from_secs(1))
        .unwrap_or_default();

    entry.kill();
    drop(listener);
    drop(router);

    eprintln!("\n=== Workspace Entry output ===\n{entry_output}");
    eprintln!("\n=== Native listener output ===\n{listener_output}");

    let received = count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(
        received >= 1,
        "Workspace Entry talker delivered no messages to the external native \
         listener (0 `Received:` lines). The Entry boots talker+listener in one \
         process; cross-process delivery on `/chatter` is the asserted signal.\n\
         Entry output:\n{entry_output}\nListener output:\n{listener_output}",
    );

    eprintln!(
        "SUCCESS: workspace Entry talker delivered {received} message(s) to the external listener"
    );
}
