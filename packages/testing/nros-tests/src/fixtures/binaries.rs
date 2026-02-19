//! Binary build helpers for integration tests
//!
//! Provides functions to build test binaries with caching support.

use crate::{TestError, TestResult, project_root};
use duct::cmd;
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};

/// Cached path to the qemu-test binary
static QEMU_TEST_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-wcet-bench binary
static QEMU_WCET_BENCH_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-lan9118 binary
static QEMU_LAN9118_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-talker binary
static NATIVE_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary
static NATIVE_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-talker binary with safety-e2e
static NATIVE_TALKER_SAFETY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary with safety-e2e
static NATIVE_LISTENER_SAFETY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary with unstable-zenoh-api (zero-copy)
static NATIVE_LISTENER_ZERO_COPY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-action-server binary
static NATIVE_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-action-client binary
static NATIVE_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-service-server binary
static NATIVE_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-service-client binary
static NATIVE_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-custom-msg binary
static NATIVE_CUSTOM_MSG_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-bsp-talker binary
static QEMU_BSP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-bsp-listener binary
static QEMU_BSP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the esp32-qemu-talker binary (ELF)
static ESP32_QEMU_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the esp32-qemu-listener binary (ELF)
static ESP32_QEMU_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-talker binary
static XRCE_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-listener binary
static XRCE_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-service-server binary
static XRCE_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-service-client binary
static XRCE_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-action-server binary
static XRCE_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-action-client binary
static XRCE_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-large-msg-test binary
static XRCE_LARGE_MSG_TEST_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the zenoh-stress-test binary
static ZENOH_STRESS_TEST_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the zenoh-stress-test binary built with large subscriber buffer
static ZENOH_STRESS_TEST_LARGE_BUF_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-stress-test binary
static XRCE_STRESS_TEST_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-bsp-large-msg-test binary
static QEMU_LARGE_MSG_TEST_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-serial-talker binary
static XRCE_SERIAL_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the xrce-serial-listener binary
static XRCE_SERIAL_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached: nros-c library built
static NROS_C_LIB: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-talker binary
static C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-listener binary
static C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-service-server binary
static C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-service-client binary
static C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-action-server binary
static C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-action-client binary
static C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-xrce-talker binary
static C_XRCE_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the c-xrce-listener binary
static C_XRCE_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build the qemu-test example and return its path
///
/// Uses OnceLock to cache the build, so subsequent calls are fast.
pub fn build_qemu_test() -> TestResult<&'static Path> {
    QEMU_TEST_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/qemu-arm/rust/core/cdr-test");

            eprintln!("Building qemu-test...");

            let output = cmd!(
                "cargo",
                "build",
                "--release",
                "--target",
                "thumbv7m-none-eabi"
            )
            .dir(&example_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let binary_path = example_dir.join("target/thumbv7m-none-eabi/release/qemu-rs-test");

            if !binary_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Binary not found after build: {}",
                    binary_path.display()
                )));
            }

            Ok(binary_path)
        })
        .map(|p| p.as_path())
}

/// Build an example from the examples directory
///
/// # Arguments
/// * `name` - Example directory name (e.g., "native-rs-talker")
/// * `binary_name` - Actual binary name (e.g., "talker")
/// * `features` - Optional features to enable
/// * `target` - Optional target triple (e.g., "thumbv7m-none-eabi")
///
/// # Returns
/// Path to the built binary
pub fn build_example(
    name: &str,
    binary_name: &str,
    features: Option<&[&str]>,
    target: Option<&str>,
) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building {}...", name);

    let mut args = vec!["build", "--release"];

    if let Some(target) = target {
        args.push("--target");
        args.push(target);
    }

    let features_str: String;
    if let Some(features) = features {
        features_str = features.join(",");
        args.push("--features");
        args.push(&features_str);
    }

    let output = cmd("cargo", &args)
        .dir(&example_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(
            String::from_utf8_lossy(&output.stdout).to_string(),
        ));
    }

    // Determine binary path using the actual binary name
    let binary_path = if let Some(target) = target {
        example_dir.join(format!("target/{}/release/{}", target, binary_name))
    } else {
        example_dir.join(format!("target/release/{}", binary_name))
    };

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

/// Build native-rs-talker with param-services feature (cached)
pub fn build_native_talker() -> TestResult<&'static Path> {
    NATIVE_TALKER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/talker",
                "talker",
                Some(&["param-services"]),
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native-rs-listener (cached)
pub fn build_native_listener() -> TestResult<&'static Path> {
    NATIVE_LISTENER_BINARY
        .get_or_try_init(|| build_example("native/rust/zenoh/listener", "listener", None, None))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-test binary path
#[rstest::fixture]
pub fn qemu_binary() -> PathBuf {
    build_qemu_test()
        .expect("Failed to build qemu-test")
        .to_path_buf()
}

/// Build the qemu-wcet-bench example and return its path (cached)
pub fn build_qemu_wcet_bench() -> TestResult<&'static Path> {
    QEMU_WCET_BENCH_BINARY
        .get_or_try_init(|| {
            build_example(
                "qemu-arm/rust/core/wcet-bench",
                "qemu-rs-wcet-bench",
                None,
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// Build the qemu-lan9118 example and return its path (cached)
pub fn build_qemu_lan9118() -> TestResult<&'static Path> {
    QEMU_LAN9118_BINARY
        .get_or_try_init(|| {
            build_example(
                "qemu-arm/rust/standalone/lan9118",
                "qemu-rs-lan9118",
                None,
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the native-rs-talker binary path
#[rstest::fixture]
pub fn talker_binary() -> PathBuf {
    build_native_talker()
        .expect("Failed to build native-rs-talker")
        .to_path_buf()
}

/// rstest fixture that provides the native-rs-listener binary path
#[rstest::fixture]
pub fn listener_binary() -> PathBuf {
    build_native_listener()
        .expect("Failed to build native-rs-listener")
        .to_path_buf()
}

/// Build native-rs-action-server (cached)
pub fn build_native_action_server() -> TestResult<&'static Path> {
    NATIVE_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/action-server",
                "native-rs-action-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native-rs-action-client (cached)
pub fn build_native_action_client() -> TestResult<&'static Path> {
    NATIVE_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/action-client",
                "native-rs-action-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native-rs-talker with safety-e2e feature (cached)
///
/// Uses a separate `target-safety` directory to avoid overwriting the
/// standard talker binary that other parallel test processes use.
pub fn build_native_talker_safety() -> TestResult<&'static Path> {
    NATIVE_TALKER_SAFETY_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/zenoh/talker");
            let target_dir = example_dir.join("target-safety");

            eprintln!("Building native/rust/zenoh/talker (safety-e2e)...");

            let output = cmd!(
                "cargo",
                "build",
                "--release",
                "--features",
                "safety-e2e",
                "--target-dir",
                target_dir.to_str().unwrap()
            )
            .dir(&example_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let binary_path = target_dir.join("release/talker");
            if !binary_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Binary not found after build: {}",
                    binary_path.display()
                )));
            }

            Ok(binary_path)
        })
        .map(|p| p.as_path())
}

/// Build native-rs-listener with safety-e2e feature (cached)
///
/// Uses a separate `target-safety` directory to avoid overwriting the
/// standard listener binary that other parallel test processes use.
pub fn build_native_listener_safety() -> TestResult<&'static Path> {
    NATIVE_LISTENER_SAFETY_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/zenoh/listener");
            let target_dir = example_dir.join("target-safety");

            eprintln!("Building native/rust/zenoh/listener (safety-e2e)...");

            let output = cmd!(
                "cargo",
                "build",
                "--release",
                "--features",
                "safety-e2e",
                "--target-dir",
                target_dir.to_str().unwrap()
            )
            .dir(&example_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let binary_path = target_dir.join("release/listener");
            if !binary_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Binary not found after build: {}",
                    binary_path.display()
                )));
            }

            Ok(binary_path)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the native-rs-talker binary path (with safety-e2e)
#[rstest::fixture]
pub fn talker_safety_binary() -> PathBuf {
    build_native_talker_safety()
        .expect("Failed to build native-rs-talker with safety-e2e")
        .to_path_buf()
}

/// rstest fixture that provides the native-rs-listener binary path (with safety-e2e)
#[rstest::fixture]
pub fn listener_safety_binary() -> PathBuf {
    build_native_listener_safety()
        .expect("Failed to build native-rs-listener with safety-e2e")
        .to_path_buf()
}

/// Build native-rs-listener with unstable-zenoh-api feature (cached)
///
/// Uses a separate `target-zero-copy` directory to avoid overwriting the
/// standard/safety listener binaries that other parallel test processes use.
pub fn build_native_listener_zero_copy() -> TestResult<&'static Path> {
    NATIVE_LISTENER_ZERO_COPY_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/zenoh/listener");
            let target_dir = example_dir.join("target-zero-copy");

            eprintln!("Building native/rust/zenoh/listener (zero-copy)...");

            let output = cmd!(
                "cargo",
                "build",
                "--release",
                "--features",
                "unstable-zenoh-api",
                "--target-dir",
                target_dir.to_str().unwrap()
            )
            .dir(&example_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let binary_path = target_dir.join("release/listener");
            if !binary_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Binary not found after build: {}",
                    binary_path.display()
                )));
            }

            Ok(binary_path)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the native-rs-action-server binary path
#[rstest::fixture]
pub fn action_server_binary() -> PathBuf {
    build_native_action_server()
        .expect("Failed to build native-rs-action-server")
        .to_path_buf()
}

/// rstest fixture that provides the native-rs-action-client binary path
#[rstest::fixture]
pub fn action_client_binary() -> PathBuf {
    build_native_action_client()
        .expect("Failed to build native-rs-action-client")
        .to_path_buf()
}

/// Build native-rs-service-server (cached)
pub fn build_native_service_server() -> TestResult<&'static Path> {
    NATIVE_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/service-server",
                "native-rs-service-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native-rs-service-client (cached)
pub fn build_native_service_client() -> TestResult<&'static Path> {
    NATIVE_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/service-client",
                "native-rs-service-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the native-rs-service-server binary path
#[rstest::fixture]
pub fn service_server_binary() -> PathBuf {
    build_native_service_server()
        .expect("Failed to build native-rs-service-server")
        .to_path_buf()
}

/// rstest fixture that provides the native-rs-service-client binary path
#[rstest::fixture]
pub fn service_client_binary() -> PathBuf {
    build_native_service_client()
        .expect("Failed to build native-rs-service-client")
        .to_path_buf()
}

/// Build native-rs-custom-msg (cached)
pub fn build_native_custom_msg() -> TestResult<&'static Path> {
    NATIVE_CUSTOM_MSG_BINARY
        .get_or_try_init(|| build_example("native/rust/zenoh/custom-msg", "custom_msg", None, None))
        .map(|p| p.as_path())
}

/// Build native-rs-custom-msg (uncached, for serialization tests)
pub fn build_native_custom_msg_no_zenoh() -> TestResult<PathBuf> {
    build_example("native/rust/zenoh/custom-msg", "custom_msg", None, None)
}

/// rstest fixture that provides the native-rs-custom-msg binary path (with zenoh)
#[rstest::fixture]
pub fn custom_msg_binary() -> PathBuf {
    build_native_custom_msg()
        .expect("Failed to build native-rs-custom-msg")
        .to_path_buf()
}

/// Build qemu-bsp-talker (cached)
pub fn build_qemu_bsp_talker() -> TestResult<&'static Path> {
    QEMU_BSP_TALKER_BINARY
        .get_or_try_init(|| {
            build_example(
                "qemu-arm/rust/zenoh/talker",
                "qemu-bsp-talker",
                None,
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// Build qemu-bsp-listener (cached)
pub fn build_qemu_bsp_listener() -> TestResult<&'static Path> {
    QEMU_BSP_LISTENER_BINARY
        .get_or_try_init(|| {
            build_example(
                "qemu-arm/rust/zenoh/listener",
                "qemu-bsp-listener",
                None,
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-bsp-talker binary path
#[rstest::fixture]
pub fn qemu_bsp_talker_binary() -> PathBuf {
    build_qemu_bsp_talker()
        .expect("Failed to build qemu-bsp-talker")
        .to_path_buf()
}

/// rstest fixture that provides the qemu-bsp-listener binary path
#[rstest::fixture]
pub fn qemu_bsp_listener_binary() -> PathBuf {
    build_qemu_bsp_listener()
        .expect("Failed to build qemu-bsp-listener")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// XRCE-DDS Example Builders
// ═══════════════════════════════════════════════════════════════════════════

/// Build the xrce-talker example binary (cached).
pub fn build_xrce_talker() -> TestResult<&'static Path> {
    XRCE_TALKER_BINARY
        .get_or_try_init(|| build_example("native/rust/xrce/talker", "xrce-talker", None, None))
        .map(|p| p.as_path())
}

/// Build the xrce-listener example binary (cached).
pub fn build_xrce_listener() -> TestResult<&'static Path> {
    XRCE_LISTENER_BINARY
        .get_or_try_init(|| build_example("native/rust/xrce/listener", "xrce-listener", None, None))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-talker binary path.
#[rstest::fixture]
pub fn xrce_talker_binary() -> PathBuf {
    build_xrce_talker()
        .expect("Failed to build xrce-talker")
        .to_path_buf()
}

/// rstest fixture that provides the xrce-listener binary path.
#[rstest::fixture]
pub fn xrce_listener_binary() -> PathBuf {
    build_xrce_listener()
        .expect("Failed to build xrce-listener")
        .to_path_buf()
}

/// Build the xrce-service-server example binary (cached).
pub fn build_xrce_service_server() -> TestResult<&'static Path> {
    XRCE_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/service-server",
                "xrce-service-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build the xrce-service-client example binary (cached).
pub fn build_xrce_service_client() -> TestResult<&'static Path> {
    XRCE_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/service-client",
                "xrce-service-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-service-server binary path.
#[rstest::fixture]
pub fn xrce_service_server_binary() -> PathBuf {
    build_xrce_service_server()
        .expect("Failed to build xrce-service-server")
        .to_path_buf()
}

/// rstest fixture that provides the xrce-service-client binary path.
#[rstest::fixture]
pub fn xrce_service_client_binary() -> PathBuf {
    build_xrce_service_client()
        .expect("Failed to build xrce-service-client")
        .to_path_buf()
}

/// Build the xrce-action-server example binary (cached).
pub fn build_xrce_action_server() -> TestResult<&'static Path> {
    XRCE_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/action-server",
                "xrce-action-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build the xrce-action-client example binary (cached).
pub fn build_xrce_action_client() -> TestResult<&'static Path> {
    XRCE_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/action-client",
                "xrce-action-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-action-server binary path.
#[rstest::fixture]
pub fn xrce_action_server_binary() -> PathBuf {
    build_xrce_action_server()
        .expect("Failed to build xrce-action-server")
        .to_path_buf()
}

/// rstest fixture that provides the xrce-action-client binary path.
#[rstest::fixture]
pub fn xrce_action_client_binary() -> PathBuf {
    build_xrce_action_client()
        .expect("Failed to build xrce-action-client")
        .to_path_buf()
}

/// Build the xrce-serial-talker example binary (cached).
pub fn build_xrce_serial_talker() -> TestResult<&'static Path> {
    XRCE_SERIAL_TALKER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/serial-talker",
                "xrce-serial-talker",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build the xrce-serial-listener example binary (cached).
pub fn build_xrce_serial_listener() -> TestResult<&'static Path> {
    XRCE_SERIAL_LISTENER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/serial-listener",
                "xrce-serial-listener",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-serial-talker binary path.
#[rstest::fixture]
pub fn xrce_serial_talker_binary() -> PathBuf {
    build_xrce_serial_talker()
        .expect("Failed to build xrce-serial-talker")
        .to_path_buf()
}

/// rstest fixture that provides the xrce-serial-listener binary path.
#[rstest::fixture]
pub fn xrce_serial_listener_binary() -> PathBuf {
    build_xrce_serial_listener()
        .expect("Failed to build xrce-serial-listener")
        .to_path_buf()
}

/// Build the xrce-large-msg-test example binary (cached).
pub fn build_xrce_large_msg_test() -> TestResult<&'static Path> {
    XRCE_LARGE_MSG_TEST_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/large-msg-test",
                "xrce-large-msg-test",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-large-msg-test binary path.
#[rstest::fixture]
pub fn xrce_large_msg_test_binary() -> PathBuf {
    build_xrce_large_msg_test()
        .expect("Failed to build xrce-large-msg-test")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// Stress Test & Large Message Builders
// ═══════════════════════════════════════════════════════════════════════════

/// Build the zenoh-stress-test binary (cached).
pub fn build_zenoh_stress_test() -> TestResult<&'static Path> {
    ZENOH_STRESS_TEST_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/zenoh/stress-test",
                "zenoh-stress-test",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the zenoh-stress-test binary path.
#[rstest::fixture]
pub fn zenoh_stress_test_binary() -> PathBuf {
    build_zenoh_stress_test()
        .expect("Failed to build zenoh-stress-test")
        .to_path_buf()
}

/// Build the zenoh-stress-test binary with large subscriber buffer (8192B, cached).
///
/// Uses `ZPICO_SUBSCRIBER_BUFFER_SIZE=8192` and a separate `target-large-buf`
/// directory to avoid overwriting the default stress-test binary.
pub fn build_zenoh_stress_test_large_buf() -> TestResult<&'static Path> {
    ZENOH_STRESS_TEST_LARGE_BUF_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/zenoh/stress-test");
            let target_dir = example_dir.join("target-large-buf");

            eprintln!("Building native/rust/zenoh/stress-test (large-buf)...");

            let output = cmd!(
                "cargo",
                "build",
                "--release",
                "--target-dir",
                target_dir.to_str().unwrap()
            )
            .env("ZPICO_SUBSCRIBER_BUFFER_SIZE", "8192")
            .dir(&example_dir)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let binary_path = target_dir.join("release/zenoh-stress-test");
            if !binary_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Binary not found: {}",
                    binary_path.display()
                )));
            }
            Ok(binary_path)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the zenoh-stress-test binary path (large subscriber buffer).
#[rstest::fixture]
pub fn zenoh_stress_test_large_buf_binary() -> PathBuf {
    build_zenoh_stress_test_large_buf()
        .expect("Failed to build zenoh-stress-test (large-buf)")
        .to_path_buf()
}

/// Build the xrce-stress-test binary (cached).
pub fn build_xrce_stress_test() -> TestResult<&'static Path> {
    XRCE_STRESS_TEST_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/xrce/stress-test",
                "xrce-stress-test",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-stress-test binary path.
#[rstest::fixture]
pub fn xrce_stress_test_binary() -> PathBuf {
    build_xrce_stress_test()
        .expect("Failed to build xrce-stress-test")
        .to_path_buf()
}

/// Build qemu-bsp-large-msg-test (cached).
pub fn build_qemu_large_msg_test() -> TestResult<&'static Path> {
    QEMU_LARGE_MSG_TEST_BINARY
        .get_or_try_init(|| {
            build_example(
                "qemu-arm/rust/zenoh/large-msg-test",
                "qemu-bsp-large-msg-test",
                None,
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-bsp-large-msg-test binary path.
#[rstest::fixture]
pub fn qemu_large_msg_test_binary() -> PathBuf {
    build_qemu_large_msg_test()
        .expect("Failed to build qemu-bsp-large-msg-test")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// C Example Builders (CMake-based)
// ═══════════════════════════════════════════════════════════════════════════

/// Build the nros-c static library (cached).
///
/// Runs `cargo build -p nros-c --release` and returns the path to `libnros_c.a`.
pub fn build_nano_ros_c_lib() -> TestResult<&'static Path> {
    NROS_C_LIB
        .get_or_try_init(|| {
            let root = project_root();

            eprintln!("Building nros-c library...");

            let output = cmd!(
                "cargo",
                "build",
                "-p",
                "nros-c",
                "--release",
                "--features",
                "rmw-zenoh,platform-posix,ros-humble"
            )
            .dir(&root)
            .stderr_to_stdout()
            .stdout_capture()
            .unchecked()
            .run()
            .map_err(|e| TestError::BuildFailed(e.to_string()))?;

            if !output.status.success() {
                return Err(TestError::BuildFailed(
                    String::from_utf8_lossy(&output.stdout).to_string(),
                ));
            }

            let lib_path = root.join("target/release/libnros_c.a");
            if !lib_path.exists() {
                return Err(TestError::BuildFailed(format!(
                    "Library not found after build: {}",
                    lib_path.display()
                )));
            }

            Ok(lib_path)
        })
        .map(|p| p.as_path())
}

/// Build a CMake-based C example.
///
/// # Arguments
/// * `example_dir` - Path relative to `examples/` (e.g., "native/c/zenoh/talker")
/// * `binary_name` - Name of the output binary (e.g., "c_talker")
///
/// This first ensures the nros-c library is built, then runs cmake + cmake --build.
pub fn build_c_example(example_dir: &str, binary_name: &str) -> TestResult<PathBuf> {
    // Ensure the C library is built first
    build_nano_ros_c_lib()?;

    let root = project_root();
    let src_dir = root.join(format!("examples/{}", example_dir));

    if !src_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "C example directory not found: {}",
            src_dir.display()
        )));
    }

    let build_dir = src_dir.join("build");

    eprintln!("Building C example {}...", example_dir);

    // Clean and create build directory
    if build_dir.exists() {
        std::fs::remove_dir_all(&build_dir)
            .map_err(|e| TestError::BuildFailed(format!("Failed to clean build dir: {}", e)))?;
    }
    std::fs::create_dir_all(&build_dir)
        .map_err(|e| TestError::BuildFailed(format!("Failed to create build dir: {}", e)))?;

    // Run cmake configure — pass NanoRos_DIR to the pseudo-install layout
    let nano_ros_dir = format!(
        "-DNanoRos_DIR={}",
        root.join("build/install/lib/cmake/NanoRos").display()
    );
    let output = cmd!("cmake", &nano_ros_dir, "..")
        .dir(&build_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake configure failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    // Run cmake build
    let output = cmd!("cmake", "--build", ".")
        .dir(&build_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake build failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let binary_path = build_dir.join(binary_name);
    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

/// Build c-talker example (cached)
pub fn build_c_talker() -> TestResult<&'static Path> {
    C_TALKER_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/talker", "c_talker"))
        .map(|p| p.as_path())
}

/// Build c-listener example (cached)
pub fn build_c_listener() -> TestResult<&'static Path> {
    C_LISTENER_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/listener", "c_listener"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the c-talker binary path
#[rstest::fixture]
pub fn c_talker_binary() -> PathBuf {
    build_c_talker()
        .expect("Failed to build c-talker")
        .to_path_buf()
}

/// rstest fixture that provides the c-listener binary path
#[rstest::fixture]
pub fn c_listener_binary() -> PathBuf {
    build_c_listener()
        .expect("Failed to build c-listener")
        .to_path_buf()
}

/// Build c-service-server example (cached)
pub fn build_c_service_server() -> TestResult<&'static Path> {
    C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/service-server", "c_service_server"))
        .map(|p| p.as_path())
}

/// Build c-service-client example (cached)
pub fn build_c_service_client() -> TestResult<&'static Path> {
    C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/service-client", "c_service_client"))
        .map(|p| p.as_path())
}

/// Build c-action-server example (cached)
pub fn build_c_action_server() -> TestResult<&'static Path> {
    C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/action-server", "c_action_server"))
        .map(|p| p.as_path())
}

/// Build c-action-client example (cached)
pub fn build_c_action_client() -> TestResult<&'static Path> {
    C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_c_example("native/c/zenoh/action-client", "c_action_client"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the c-service-server binary path
#[rstest::fixture]
pub fn c_service_server_binary() -> PathBuf {
    build_c_service_server()
        .expect("Failed to build c-service-server")
        .to_path_buf()
}

/// rstest fixture that provides the c-service-client binary path
#[rstest::fixture]
pub fn c_service_client_binary() -> PathBuf {
    build_c_service_client()
        .expect("Failed to build c-service-client")
        .to_path_buf()
}

/// rstest fixture that provides the c-action-server binary path
#[rstest::fixture]
pub fn c_action_server_binary() -> PathBuf {
    build_c_action_server()
        .expect("Failed to build c-action-server")
        .to_path_buf()
}

/// rstest fixture that provides the c-action-client binary path
#[rstest::fixture]
pub fn c_action_client_binary() -> PathBuf {
    build_c_action_client()
        .expect("Failed to build c-action-client")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// C XRCE Example Builders (CMake-based, XRCE-DDS backend)
// ═══════════════════════════════════════════════════════════════════════════

/// Build a CMake-based C example that uses the XRCE backend.
///
/// Similar to `build_c_example()` but passes `-DNANO_ROS_RMW=xrce` to select
/// the pre-installed XRCE library variant (`libnros_c_xrce.a`).
pub fn build_c_xrce_example(example_dir: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let src_dir = root.join(format!("examples/{}", example_dir));

    if !src_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "C XRCE example directory not found: {}",
            src_dir.display()
        )));
    }

    let build_dir = src_dir.join("build");

    eprintln!("Building C XRCE example {}...", example_dir);

    // Clean and create build directory
    if build_dir.exists() {
        std::fs::remove_dir_all(&build_dir)
            .map_err(|e| TestError::BuildFailed(format!("Failed to clean build dir: {}", e)))?;
    }
    std::fs::create_dir_all(&build_dir)
        .map_err(|e| TestError::BuildFailed(format!("Failed to create build dir: {}", e)))?;

    // Run cmake configure — select XRCE RMW variant
    let nano_ros_dir = format!(
        "-DNanoRos_DIR={}",
        root.join("build/install/lib/cmake/NanoRos").display()
    );
    let output = cmd!("cmake", &nano_ros_dir, "-DNANO_ROS_RMW=xrce", "..")
        .dir(&build_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake configure failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    // Run cmake build
    let output = cmd!("cmake", "--build", ".")
        .dir(&build_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake build failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let binary_path = build_dir.join(binary_name);
    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

/// Build c-xrce-talker example (cached)
pub fn build_c_xrce_talker() -> TestResult<&'static Path> {
    C_XRCE_TALKER_BINARY
        .get_or_try_init(|| build_c_xrce_example("native/c/xrce/talker", "c_xrce_talker"))
        .map(|p| p.as_path())
}

/// Build c-xrce-listener example (cached)
pub fn build_c_xrce_listener() -> TestResult<&'static Path> {
    C_XRCE_LISTENER_BINARY
        .get_or_try_init(|| build_c_xrce_example("native/c/xrce/listener", "c_xrce_listener"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the c-xrce-talker binary path
#[rstest::fixture]
pub fn c_xrce_talker_binary() -> PathBuf {
    build_c_xrce_talker()
        .expect("Failed to build c-xrce-talker")
        .to_path_buf()
}

/// rstest fixture that provides the c-xrce-listener binary path
#[rstest::fixture]
pub fn c_xrce_listener_binary() -> PathBuf {
    build_c_xrce_listener()
        .expect("Failed to build c-xrce-listener")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// ESP32-C3 QEMU Example Builders (nightly toolchain)
// ═══════════════════════════════════════════════════════════════════════════

/// Build an ESP32-C3 QEMU example using `cargo +nightly`
///
/// ESP32 examples require the nightly toolchain with `-Zbuild-std`, so we
/// can't use the generic `build_example()` which uses stable `cargo build`.
fn build_esp32_qemu_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-esp32/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ESP32 example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-esp32/rust/zenoh/{}...", name);

    let output = cmd!("cargo", "+nightly", "build", "--release")
        .dir(&example_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(
            String::from_utf8_lossy(&output.stdout).to_string(),
        ));
    }

    let binary_path = example_dir.join(format!(
        "target/riscv32imc-unknown-none-elf/release/{}",
        binary_name
    ));

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

/// Build esp32-qemu-talker (cached)
pub fn build_esp32_qemu_talker() -> TestResult<&'static Path> {
    ESP32_QEMU_TALKER_BINARY
        .get_or_try_init(|| build_esp32_qemu_example("talker", "esp32-qemu-talker"))
        .map(|p| p.as_path())
}

/// Build esp32-qemu-listener (cached)
pub fn build_esp32_qemu_listener() -> TestResult<&'static Path> {
    ESP32_QEMU_LISTENER_BINARY
        .get_or_try_init(|| build_esp32_qemu_example("listener", "esp32-qemu-listener"))
        .map(|p| p.as_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_root_has_examples() {
        let root = project_root();
        assert!(root.join("examples").exists());
    }
}
