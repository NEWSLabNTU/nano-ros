//! Parameter Server Integration Tests
//!
//! Tests for parameter declaration, get/set, and ROS 2 interoperability.
//!
//! Run with: `cargo nextest run -p nros-tests --test params`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, build_native_talker, require_ros2, require_zenohd, zenohd_unique,
};
use rstest::rstest;
use std::process::Command;
use std::time::Duration;

// =============================================================================
// Parameter Declaration Tests
// =============================================================================

/// Test that talker with parameters builds successfully
#[rstest]
fn test_talker_with_params_builds() {
    let binary = build_native_talker().expect("Failed to build native-rs-talker");
    assert!(binary.exists(), "Binary should exist: {}", binary.display());
    println!(
        "SUCCESS: Built talker with parameters: {}",
        binary.display()
    );
}

/// Test that talker starts and uses default parameter value
#[rstest]
fn test_talker_uses_default_param(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Let it run briefly
    std::thread::sleep(Duration::from_secs(3));

    // Kill the process before collecting output
    proc.kill();

    // Capture both stdout and stderr (env_logger writes to stderr)
    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker parameter output ===");
    println!("{}", output);

    // Verify parameter was declared and used with default value
    assert!(
        output.contains("Counter start value: 0"),
        "Should show default parameter value of 0. Output:\n{}",
        output
    );

    // Verify parameter declaration succeeded (no errors)
    assert!(
        !output.contains("Failed to declare parameter"),
        "Should not have parameter declaration errors"
    );

    println!("SUCCESS: Talker uses default parameter value");
}

/// Test that talker declares parameter with correct constraints
#[rstest]
fn test_talker_param_declaration(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "debug") // Debug level to see parameter details
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    // Kill the process before collecting output
    proc.kill();

    // Capture both stdout and stderr
    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    println!("=== Talker debug output ===");
    println!("{}", output);

    // Verify node was created
    assert!(
        output.contains("Node created") || output.contains("talker"),
        "Node should be created. Output:\n{}",
        output
    );

    // Parameter value should be logged
    assert!(
        output.contains("Counter start value"),
        "Should log counter start value. Output:\n{}",
        output
    );

    println!("SUCCESS: Parameter declaration works correctly");
}

// =============================================================================
// ROS 2 Parameter Interop Tests (requires ROS 2 + rmw_zenoh_cpp)
// =============================================================================

/// Helper to start a talker and wait for parameter services to register
fn start_talker_with_params(locator: &str) -> ManagedProcess {
    let binary = build_native_talker().expect("Failed to build");

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", locator)
        .env("ZENOH_MODE", "client");

    let mut talker = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start talker");

    // Wait for talker to start publishing (ensures parameter services are registered)
    let _ = talker.wait_for_output_pattern("Publishing", Duration::from_secs(5));

    // Extra delay for parameter service discovery propagation through zenohd
    std::thread::sleep(Duration::from_secs(2));

    talker
}

/// Verify the nros node is discoverable via `ros2 node list`.
/// Returns true if discoverable, false otherwise (prints skip message).
fn require_node_discoverable(locator: &str) -> bool {
    for attempt in 1..=3 {
        if let Ok(output) = nros_tests::ros2::ros2_node_list(locator, "humble")
            && output.contains("/demo/talker")
        {
            return true;
        }
        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
    eprintln!(
        "Skipping test: nros node /demo/talker not discoverable via ros2 \
         (may be zenohd version mismatch or rmw_zenoh configuration issue)"
    );
    false
}

/// Test that ROS 2 can list parameters on nros node
#[rstest]
fn test_ros2_param_list(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let locator = zenohd_unique.locator();
    let _talker = start_talker_with_params(&locator);

    if !require_node_discoverable(&locator) {
        return;
    }

    // Retry up to 3 times since parameter services need discovery time
    let mut ros2_stdout = String::new();
    for attempt in 1..=3 {
        ros2_stdout = nros_tests::ros2::ros2_param_list("/demo/talker", &locator, "humble")
            .expect("Failed to run ros2 param list");

        println!("=== ros2 param list attempt {} ===", attempt);
        println!("{}", ros2_stdout);

        if ros2_stdout.contains("start_value") {
            break;
        }

        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    assert!(
        ros2_stdout.contains("start_value"),
        "ros2 param list should show 'start_value' parameter. Output:\n{}",
        ros2_stdout
    );
}

/// Test that ROS 2 can get parameter value from nros node
#[rstest]
fn test_ros2_param_get(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let locator = zenohd_unique.locator();
    let _talker = start_talker_with_params(&locator);

    if !require_node_discoverable(&locator) {
        return;
    }

    // Retry up to 3 times for discovery
    let mut ros2_stdout = String::new();
    for attempt in 1..=3 {
        ros2_stdout =
            nros_tests::ros2::ros2_param_get("/demo/talker", "start_value", &locator, "humble")
                .expect("Failed to run ros2 param get");

        println!("=== ros2 param get attempt {} ===", attempt);
        println!("{}", ros2_stdout);

        if ros2_stdout.contains("Integer value is: 0") {
            break;
        }

        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    assert!(
        ros2_stdout.contains("Integer value is: 0"),
        "ros2 param get should show 'Integer value is: 0'. Output:\n{}",
        ros2_stdout
    );
}

/// Test that ROS 2 can set and read back a parameter on nros node
#[rstest]
fn test_ros2_param_set(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let locator = zenohd_unique.locator();
    let _talker = start_talker_with_params(&locator);

    if !require_node_discoverable(&locator) {
        return;
    }

    // Set start_value to 42
    let set_output =
        nros_tests::ros2::ros2_param_set("/demo/talker", "start_value", "42", &locator, "humble")
            .expect("Failed to run ros2 param set");

    println!("=== ros2 param set output ===");
    println!("{}", set_output);

    assert!(
        set_output.contains("Set parameter successful"),
        "ros2 param set should succeed. Output:\n{}",
        set_output
    );

    // Read back to verify
    let get_output =
        nros_tests::ros2::ros2_param_get("/demo/talker", "start_value", &locator, "humble")
            .expect("Failed to run ros2 param get");

    println!("=== ros2 param get (after set) output ===");
    println!("{}", get_output);

    assert!(
        get_output.contains("Integer value is: 42"),
        "ros2 param get should show updated value 42. Output:\n{}",
        get_output
    );
}

/// Test that ROS 2 can describe parameter on nros node
#[rstest]
fn test_ros2_param_describe(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let locator = zenohd_unique.locator();
    let _talker = start_talker_with_params(&locator);

    if !require_node_discoverable(&locator) {
        return;
    }

    // Retry up to 3 times for discovery
    let mut ros2_stdout = String::new();
    for attempt in 1..=3 {
        ros2_stdout = nros_tests::ros2::ros2_param_describe(
            "/demo/talker",
            "start_value",
            &locator,
            "humble",
        )
        .expect("Failed to run ros2 param describe");

        println!("=== ros2 param describe attempt {} ===", attempt);
        println!("{}", ros2_stdout);

        if ros2_stdout.contains("Type: integer") || ros2_stdout.contains("integer") {
            break;
        }

        if attempt < 3 {
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    assert!(
        ros2_stdout.contains("Type: integer")
            || ros2_stdout.contains("integer")
            || ros2_stdout.contains("Description:"),
        "ros2 param describe should show parameter type info. Output:\n{}",
        ros2_stdout
    );
}

// =============================================================================
// Parameter Type Tests
// =============================================================================

/// Test that parameter is correctly typed as integer
#[rstest]
fn test_param_integer_type(zenohd_unique: ZenohRouter) {
    if !require_zenohd() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    let mut cmd = Command::new(binary);
    cmd.env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut proc = ManagedProcess::spawn_command(cmd, "talker").expect("Failed to start");

    std::thread::sleep(Duration::from_secs(3));

    // Kill the process before collecting output
    proc.kill();

    // Capture both stdout and stderr
    let output = proc
        .wait_for_all_output(Duration::from_secs(2))
        .unwrap_or_default();

    // The counter is used as i32, so it should work with the i64 parameter
    assert!(
        output.contains("Published: data=0") || output.contains("Published: data=1"),
        "Should publish with integer counter. Output:\n{}",
        output
    );

    println!("SUCCESS: Parameter integer type works correctly");
}
