//! Integration tests for rosidl-bindgen
//!
//! NOTE: These tests are currently disabled because rosidl-bindgen is now a library-only crate.
//! The binary functionality has been moved to cargo-ros2.
//! Tests for the `cargo ros2 bindgen` command should be in cargo-ros2/tests/ instead.

// Temporarily disabled - tests were for old cargo-ros2-bindgen binary
// TODO: Move these tests to cargo-ros2/tests/ and test `cargo ros2 bindgen` command

/*
use std::fs;
use std::path::PathBuf;
use std::process::Command;

/// Helper to create a test ROS 2 package
fn create_test_ros_package(temp_dir: &std::path::Path, package_name: &str) -> PathBuf {
    let share_dir = temp_dir.join("share").join(package_name);

    // Create msg directory with a simple message
    let msg_dir = share_dir.join("msg");
    fs::create_dir_all(&msg_dir).unwrap();
    fs::write(
        msg_dir.join("Point.msg"),
        "float64 x\nfloat64 y\nfloat64 z\n",
    )
    .unwrap();

    // Create srv directory with a simple service
    let srv_dir = share_dir.join("srv");
    fs::create_dir_all(&srv_dir).unwrap();
    fs::write(
        srv_dir.join("AddTwoInts.srv"),
        "int64 a\nint64 b\n---\nint64 sum\n",
    )
    .unwrap();

    // Create action directory with a simple action
    let action_dir = share_dir.join("action");
    fs::create_dir_all(&action_dir).unwrap();
    fs::write(
        action_dir.join("Fibonacci.action"),
        "int32 order\n---\nint32[] sequence\n---\nint32[] partial_sequence\n",
    )
    .unwrap();

    share_dir
}

#[test]
fn test_end_to_end_package_generation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let share_dir = create_test_ros_package(temp_dir.path(), "test_msgs");
    let output_dir = temp_dir.path().join("output");

    // Run cargo-ros2-bindgen via command
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-ros2-bindgen"));
    cmd.arg("--package")
        .arg("test_msgs")
        .arg("--output")
        .arg(&output_dir)
        .arg("--package-path")
        .arg(&share_dir);

    let output = cmd.output().expect("Failed to run cargo-ros2-bindgen");

    assert!(
        output.status.success(),
        "Command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output structure
    let pkg_dir = output_dir.join("test_msgs");
    assert!(pkg_dir.exists(), "Package directory should exist");
    assert!(
        pkg_dir.join("Cargo.toml").exists(),
        "Cargo.toml should exist"
    );
    assert!(pkg_dir.join("build.rs").exists(), "build.rs should exist");
    assert!(
        pkg_dir.join("src").join("lib.rs").exists(),
        "lib.rs should exist"
    );

    // Verify generated message files
    let msg_dir = pkg_dir.join("src").join("msg");
    assert!(msg_dir.exists(), "msg directory should exist");
    assert!(
        msg_dir.join("point_idiomatic.rs").exists(),
        "Idiomatic message should exist"
    );

    // Verify FFI directory structure at package level
    let ffi_msg_dir = pkg_dir.join("src").join("ffi").join("msg");
    assert!(ffi_msg_dir.exists(), "ffi/msg directory should exist");
    assert!(
        ffi_msg_dir.join("point_rmw.rs").exists(),
        "FFI message should exist in ffi/msg subdirectory"
    );

    // Verify generated service files
    let srv_dir = pkg_dir.join("src").join("srv");
    assert!(srv_dir.exists(), "srv directory should exist");
    assert!(
        srv_dir.join("add_two_ints_idiomatic.rs").exists(),
        "Idiomatic service should exist"
    );

    let ffi_srv_dir = pkg_dir.join("src").join("ffi").join("srv");
    assert!(ffi_srv_dir.exists(), "ffi/srv directory should exist");
    assert!(
        ffi_srv_dir.join("add_two_ints_rmw.rs").exists(),
        "FFI service should exist in ffi/srv subdirectory"
    );

    // Verify generated action files
    let action_dir = pkg_dir.join("src").join("action");
    assert!(action_dir.exists(), "action directory should exist");
    assert!(
        action_dir.join("fibonacci_idiomatic.rs").exists(),
        "Idiomatic action should exist"
    );

    let ffi_action_dir = pkg_dir.join("src").join("ffi").join("action");
    assert!(ffi_action_dir.exists(), "ffi/action directory should exist");
    assert!(
        ffi_action_dir.join("fibonacci_rmw.rs").exists(),
        "FFI action should exist in ffi/action subdirectory"
    );
}

#[test]
fn test_generated_package_compiles() {
    let temp_dir = tempfile::tempdir().unwrap();
    let share_dir = create_test_ros_package(temp_dir.path(), "compile_test");
    let output_dir = temp_dir.path().join("output");

    // Generate bindings
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-ros2-bindgen"));
    cmd.arg("--package")
        .arg("compile_test")
        .arg("--output")
        .arg(&output_dir)
        .arg("--package-path")
        .arg(&share_dir);

    let output = cmd.output().expect("Failed to run cargo-ros2-bindgen");
    assert!(output.status.success());

    // Try to check the generated package (don't build, just check syntax)
    // This is a basic smoke test - full compilation would require ROS 2 C libraries
    let pkg_dir = output_dir.join("compile_test");
    let check_output = Command::new("cargo")
        .arg("check")
        .arg("--manifest-path")
        .arg(pkg_dir.join("Cargo.toml"))
        .output();

    // Note: This will fail without ROS 2 C libraries installed, which is expected
    // The test passes if the command runs (even if it fails due to missing libs)
    assert!(
        check_output.is_ok(),
        "cargo check should at least run (may fail due to missing ROS libs)"
    );
}

#[test]
fn test_verbose_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let share_dir = create_test_ros_package(temp_dir.path(), "verbose_test");
    let output_dir = temp_dir.path().join("output");

    // Run with verbose flag
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-ros2-bindgen"));
    cmd.arg("--package")
        .arg("verbose_test")
        .arg("--output")
        .arg(&output_dir)
        .arg("--package-path")
        .arg(&share_dir)
        .arg("--verbose");

    let output = cmd.output().expect("Failed to run cargo-ros2-bindgen");
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cargo-ros2-bindgen starting"),
        "Verbose output should contain startup message"
    );
    assert!(
        stderr.contains("Messages:"),
        "Verbose output should contain message count"
    );
    assert!(
        stderr.contains("Services:"),
        "Verbose output should contain service count"
    );
    assert!(
        stderr.contains("Actions:"),
        "Verbose output should contain action count"
    );
}
*/
