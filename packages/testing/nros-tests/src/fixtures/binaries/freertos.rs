//! FreeRTOS QEMU MPS2-AN385 binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the FreeRTOS Rust / C / C++
//! examples. Moved out of `tests/freertos_qemu.rs` so the same caches
//! can be reused across multiple test files in a single nextest run
//! (see `docs/roadmap/phase-85-test-suite-consolidation.md`, 85.5).

use crate::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

// =============================================================================
// Prerequisite detection
// =============================================================================

/// `FREERTOS_DIR` env var set and points to a valid kernel source tree.
pub fn is_freertos_available() -> bool {
    std::env::var("FREERTOS_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("tasks.c").exists())
        .unwrap_or(false)
}

/// `LWIP_DIR` env var set and points to a valid lwIP source tree.
pub fn is_lwip_available() -> bool {
    std::env::var("LWIP_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("src/include/lwip/init.h").exists())
        .unwrap_or(false)
}

/// `arm-none-eabi-gcc` in PATH.
pub fn is_arm_gcc_available() -> bool {
    Command::new("arm-none-eabi-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
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

static FREERTOS_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-freertos/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "FreeRTOS example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path =
        example_dir.join(format!("target/thumbv7m-none-eabi/release/{}", binary_name));

    // Tests must not compile fixtures — run `just build-test-fixtures` first.
    super::require_prebuilt_binary(&binary_path)
}

pub fn build_freertos_talker() -> TestResult<&'static Path> {
    FREERTOS_TALKER_BINARY
        .get_or_try_init(|| build_rust_example("talker", "qemu-freertos-talker"))
        .map(|p| p.as_path())
}

pub fn build_freertos_listener() -> TestResult<&'static Path> {
    FREERTOS_LISTENER_BINARY
        .get_or_try_init(|| build_rust_example("listener", "qemu-freertos-listener"))
        .map(|p| p.as_path())
}

pub fn build_freertos_service_server() -> TestResult<&'static Path> {
    FREERTOS_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("service-server", "qemu-freertos-service-server"))
        .map(|p| p.as_path())
}

pub fn build_freertos_service_client() -> TestResult<&'static Path> {
    FREERTOS_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("service-client", "qemu-freertos-service-client"))
        .map(|p| p.as_path())
}

pub fn build_freertos_action_server() -> TestResult<&'static Path> {
    FREERTOS_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("action-server", "qemu-freertos-action-server"))
        .map(|p| p.as_path())
}

pub fn build_freertos_action_client() -> TestResult<&'static Path> {
    FREERTOS_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("action-client", "qemu-freertos-action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// FreeRTOS DDS variant (Phase 97.4.freertos)
// =============================================================================

static FREERTOS_DDS_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_DDS_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_dds_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-freertos/rust/dds/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "FreeRTOS DDS example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path =
        example_dir.join(format!("target/thumbv7m-none-eabi/release/{}", binary_name));

    super::require_prebuilt_binary(&binary_path)
}

pub fn build_freertos_dds_talker() -> TestResult<&'static Path> {
    FREERTOS_DDS_TALKER_BINARY
        .get_or_try_init(|| build_dds_rust_example("talker", "qemu-freertos-dds-talker"))
        .map(|p| p.as_path())
}

pub fn build_freertos_dds_listener() -> TestResult<&'static Path> {
    FREERTOS_DDS_LISTENER_BINARY
        .get_or_try_init(|| build_dds_rust_example("listener", "qemu-freertos-dds-listener"))
        .map(|p| p.as_path())
}

// =============================================================================
// C / C++ binary builders (CMake)
// =============================================================================

static FREERTOS_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static FREERTOS_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static FREERTOS_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build a FreeRTOS CMake example (C or C++).
fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!(
        "examples/qemu-arm-freertos/{}/zenoh/{}",
        lang, name
    ));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "FreeRTOS {} example directory not found: {}",
            lang,
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build");
    let binary_path = build_dir.join(binary_name);
    super::require_prebuilt_binary(&binary_path)
}

pub fn build_freertos_cpp_talker() -> TestResult<&'static Path> {
    FREERTOS_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "freertos_cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_freertos_cpp_listener() -> TestResult<&'static Path> {
    FREERTOS_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "freertos_cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_freertos_cpp_service_server() -> TestResult<&'static Path> {
    FREERTOS_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-server", "freertos_cpp_service_server")
        })
        .map(|p| p.as_path())
}

pub fn build_freertos_cpp_service_client() -> TestResult<&'static Path> {
    FREERTOS_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-client", "freertos_cpp_service_client")
        })
        .map(|p| p.as_path())
}

pub fn build_freertos_cpp_action_server() -> TestResult<&'static Path> {
    FREERTOS_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-server", "freertos_cpp_action_server")
        })
        .map(|p| p.as_path())
}

pub fn build_freertos_cpp_action_client() -> TestResult<&'static Path> {
    FREERTOS_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-client", "freertos_cpp_action_client")
        })
        .map(|p| p.as_path())
}

pub fn build_freertos_c_talker() -> TestResult<&'static Path> {
    FREERTOS_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "freertos_c_talker"))
        .map(|p| p.as_path())
}

pub fn build_freertos_c_listener() -> TestResult<&'static Path> {
    FREERTOS_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "freertos_c_listener"))
        .map(|p| p.as_path())
}

pub fn build_freertos_c_service_server() -> TestResult<&'static Path> {
    FREERTOS_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "freertos_c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_freertos_c_service_client() -> TestResult<&'static Path> {
    FREERTOS_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "freertos_c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_freertos_c_action_server() -> TestResult<&'static Path> {
    FREERTOS_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "freertos_c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_freertos_c_action_client() -> TestResult<&'static Path> {
    FREERTOS_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "freertos_c_action_client"))
        .map(|p| p.as_path())
}
