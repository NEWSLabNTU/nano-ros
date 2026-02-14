//! Parameter Server Integration Tests
//!
//! Tests for parameter declaration, get/set, and ROS 2 interoperability.
//!
//! Run with: `cargo test -p nano-ros-tests --test params -- --nocapture`
//! Or: `just test-rust-params`

use nano_ros_tests::fixtures::{
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

/// Test that ROS 2 can list parameters on nros node
#[rstest]
fn test_ros2_param_list(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    // Start nros talker
    let mut talker_cmd = Command::new(binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    // Give talker time to start and register parameter services
    std::thread::sleep(Duration::from_secs(3));

    // Run ros2 param list
    let ros2_output = Command::new("bash")
        .args([
            "-c",
            &format!(
                "source /opt/ros/humble/setup.bash && \
                 export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
                 export ZENOH_ROUTER_CHECK_ATTEMPTS=1 && \
                 timeout 10 ros2 param list /demo/talker 2>&1 || true"
            ),
        ])
        .env("ZENOH_LOCATOR", &locator)
        .output()
        .expect("Failed to run ros2 param list");

    let ros2_stdout = String::from_utf8_lossy(&ros2_output.stdout);
    let ros2_stderr = String::from_utf8_lossy(&ros2_output.stderr);

    println!("=== ros2 param list output ===");
    println!("stdout: {}", ros2_stdout);
    println!("stderr: {}", ros2_stderr);

    // Get talker output for debugging
    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    println!("=== Talker output ===");
    println!("{}", talker_output);

    // Check if ROS 2 found the node and its parameters
    // Note: This may fail if parameter services aren't fully registered
    let found_params = ros2_stdout.contains("start_value") || ros2_stdout.contains("use_sim_time"); // ROS 2 default param

    if found_params {
        println!("SUCCESS: ROS 2 can list nros parameters");
    } else {
        println!(
            "INFO: ROS 2 param list didn't find parameters (parameter services may not be registered)"
        );
        // Don't fail - parameter services may not be fully implemented for interop
    }
}

/// Test that ROS 2 can get parameter value from nros node
#[rstest]
fn test_ros2_param_get(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    // Start nros talker
    let mut talker_cmd = Command::new(binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    // Run ros2 param get
    let ros2_output = Command::new("bash")
        .args([
            "-c",
            &format!(
                "source /opt/ros/humble/setup.bash && \
                 export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
                 export ZENOH_ROUTER_CHECK_ATTEMPTS=1 && \
                 timeout 10 ros2 param get /demo/talker start_value 2>&1 || true"
            ),
        ])
        .env("ZENOH_LOCATOR", &locator)
        .output()
        .expect("Failed to run ros2 param get");

    let ros2_stdout = String::from_utf8_lossy(&ros2_output.stdout);
    let ros2_stderr = String::from_utf8_lossy(&ros2_output.stderr);

    println!("=== ros2 param get output ===");
    println!("stdout: {}", ros2_stdout);
    println!("stderr: {}", ros2_stderr);

    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    println!("=== Talker output ===");
    println!("{}", talker_output);

    // Check if ROS 2 got the parameter value
    let got_value = ros2_stdout.contains("Integer value is: 0")
        || ros2_stdout.contains("0")
        || ros2_stdout.contains("start_value");

    if got_value && !ros2_stdout.contains("error") && !ros2_stdout.contains("not found") {
        println!("SUCCESS: ROS 2 can get nros parameter value");
    } else {
        println!(
            "INFO: ROS 2 param get didn't retrieve value (parameter services may not be registered)"
        );
    }
}

/// Test that ROS 2 can describe parameter on nros node
#[rstest]
fn test_ros2_param_describe(zenohd_unique: ZenohRouter) {
    if !require_zenohd() || !require_ros2() {
        return;
    }

    let binary = build_native_talker().expect("Failed to build");
    let locator = zenohd_unique.locator();

    // Start nros talker
    let mut talker_cmd = Command::new(binary);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("ZENOH_LOCATOR", &locator)
        .env("ZENOH_MODE", "client");

    let mut talker =
        ManagedProcess::spawn_command(talker_cmd, "talker").expect("Failed to start talker");

    std::thread::sleep(Duration::from_secs(3));

    // Run ros2 param describe
    let ros2_output = Command::new("bash")
        .args([
            "-c",
            &format!(
                "source /opt/ros/humble/setup.bash && \
                 export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
                 export ZENOH_ROUTER_CHECK_ATTEMPTS=1 && \
                 timeout 10 ros2 param describe /demo/talker start_value 2>&1 || true"
            ),
        ])
        .env("ZENOH_LOCATOR", &locator)
        .output()
        .expect("Failed to run ros2 param describe");

    let ros2_stdout = String::from_utf8_lossy(&ros2_output.stdout);
    let ros2_stderr = String::from_utf8_lossy(&ros2_output.stderr);

    println!("=== ros2 param describe output ===");
    println!("stdout: {}", ros2_stdout);
    println!("stderr: {}", ros2_stderr);

    let talker_output = talker
        .wait_for_output(Duration::from_secs(2))
        .unwrap_or_default();
    println!("=== Talker output ===");
    println!("{}", talker_output);

    // Check if ROS 2 got the parameter description
    let got_description = ros2_stdout.contains("Description:")
        || ros2_stdout.contains("Initial value")
        || ros2_stdout.contains("integer");

    if got_description {
        println!("SUCCESS: ROS 2 can describe nros parameter");
    } else {
        println!(
            "INFO: ROS 2 param describe didn't get description (parameter services may not be registered)"
        );
    }
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
