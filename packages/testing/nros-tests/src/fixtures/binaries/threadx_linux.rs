//! ThreadX Linux (native simulation) binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the ThreadX-Linux Rust / C /
//! C++ examples. Moved out of `tests/threadx_linux.rs` (Phase 85.5).

use crate::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;

// =============================================================================
// Prerequisite detection
// =============================================================================

/// `THREADX_DIR` env var set and points to a valid kernel source tree.
pub fn is_threadx_available() -> bool {
    std::env::var("THREADX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("common/inc/tx_api.h").exists())
        .unwrap_or(false)
}

/// nsos-netx BSD shim source is available at the expected repo location.
pub fn is_nsos_netx_available() -> bool {
    let root = project_root();
    root.join("packages/drivers/nsos-netx/src/nsos_netx.c")
        .exists()
}

/// `cmake` in PATH (for C / C++ examples).
pub fn is_cmake_available() -> bool {
    Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// =============================================================================
// Rust binary builders
// =============================================================================

static THREADX_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/threadx-linux/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX Linux example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!("target/release/{}", binary_name));

    // Default contract: tests don't compile fixtures.
    if let Some(result) = super::require_prebuilt_binary(&binary_path) {
        return result;
    }

    eprintln!("Building threadx-linux/rust/zenoh/{}...", name);

    let output = duct::cmd!("cargo", "build", "--release")
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

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

pub fn build_threadx_talker() -> TestResult<&'static Path> {
    THREADX_TALKER_BINARY
        .get_or_try_init(|| build_rust_example("talker", "threadx-linux-talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_listener() -> TestResult<&'static Path> {
    THREADX_LISTENER_BINARY
        .get_or_try_init(|| build_rust_example("listener", "threadx-linux-listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_service_server() -> TestResult<&'static Path> {
    THREADX_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("service-server", "threadx-linux-service-server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_service_client() -> TestResult<&'static Path> {
    THREADX_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("service-client", "threadx-linux-service-client"))
        .map(|p| p.as_path())
}

pub fn build_threadx_action_server() -> TestResult<&'static Path> {
    THREADX_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("action-server", "threadx-linux-action-server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_action_client() -> TestResult<&'static Path> {
    THREADX_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("action-client", "threadx-linux-action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// C / C++ binary builders (CMake)
// =============================================================================

static THREADX_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
// C++ action builders are kept against a future Phase 69.7 follow-up.
#[allow(dead_code)]
static THREADX_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
#[allow(dead_code)]
static THREADX_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static THREADX_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/threadx-linux/{}/zenoh/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX {lang} example directory not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build");
    let binary_path = build_dir.join(binary_name);

    // Default contract: tests don't compile fixtures.
    if let Some(result) = super::require_prebuilt_binary(&binary_path) {
        return result;
    }

    eprintln!("Building threadx-linux/{}/zenoh/{} (CMake)...", lang, name);

    std::fs::create_dir_all(&build_dir).ok();

    let prefix_path = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let threadx_dir = std::env::var("THREADX_DIR").unwrap_or_else(|_| {
        root.join("third-party/threadx/kernel")
            .display()
            .to_string()
    });
    let nsos_netx_dir = std::env::var("NSOS_NETX_DIR").unwrap_or_else(|_| {
        root.join("packages/drivers/nsos-netx")
            .display()
            .to_string()
    });
    let config_dir = std::env::var("THREADX_CONFIG_DIR").unwrap_or_else(|_| {
        root.join("packages/boards/nros-threadx-linux/config")
            .display()
            .to_string()
    });
    let app_define = root
        .join("packages/boards/nros-threadx-linux/c/app_define.c")
        .display()
        .to_string();

    let output = duct::cmd!(
        "cmake",
        "-S",
        &example_dir,
        "-B",
        &build_dir,
        &prefix_path,
        "-DNANO_ROS_PLATFORM=threadx_linux",
        &format!("-DTHREADX_DIR={threadx_dir}"),
        &format!("-DNSOS_NETX_DIR={nsos_netx_dir}"),
        &format!("-DTHREADX_CONFIG_DIR={config_dir}"),
        &format!("-DTHREADX_APP_DEFINE={app_define}"),
        "-DCMAKE_BUILD_TYPE=Release"
    )
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run()
    .map_err(|e| TestError::BuildFailed(format!("cmake configure: {}", e)))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake configure failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let output = duct::cmd!("cmake", "--build", &build_dir)
        .stderr_to_stdout()
        .stdout_capture()
        .unchecked()
        .run()
        .map_err(|e| TestError::BuildFailed(format!("cmake build: {}", e)))?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "cmake build failed:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

pub fn build_threadx_cpp_talker() -> TestResult<&'static Path> {
    THREADX_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "threadx_cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_listener() -> TestResult<&'static Path> {
    THREADX_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "threadx_cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_service_server() -> TestResult<&'static Path> {
    THREADX_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-server", "threadx_cpp_service_server")
        })
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_service_client() -> TestResult<&'static Path> {
    THREADX_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-client", "threadx_cpp_service_client")
        })
        .map(|p| p.as_path())
}

#[allow(dead_code)]
pub fn build_threadx_cpp_action_server() -> TestResult<&'static Path> {
    THREADX_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-server", "threadx_cpp_action_server")
        })
        .map(|p| p.as_path())
}

#[allow(dead_code)]
pub fn build_threadx_cpp_action_client() -> TestResult<&'static Path> {
    THREADX_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-client", "threadx_cpp_action_client")
        })
        .map(|p| p.as_path())
}

pub fn build_threadx_c_talker() -> TestResult<&'static Path> {
    THREADX_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "threadx_c_talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_listener() -> TestResult<&'static Path> {
    THREADX_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "threadx_c_listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_service_server() -> TestResult<&'static Path> {
    THREADX_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "threadx_c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_service_client() -> TestResult<&'static Path> {
    THREADX_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "threadx_c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_action_server() -> TestResult<&'static Path> {
    THREADX_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "threadx_c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_action_client() -> TestResult<&'static Path> {
    THREADX_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "threadx_c_action_client"))
        .map(|p| p.as_path())
}
