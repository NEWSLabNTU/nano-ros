//! ThreadX QEMU RISC-V 64-bit binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the ThreadX-RISC-V Rust / C /
//! C++ examples. Moved out of `tests/threadx_riscv64_qemu.rs` (Phase 85.5).

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

/// `NETX_DIR` env var set and points to a valid NetX Duo source tree.
pub fn is_netx_available() -> bool {
    std::env::var("NETX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("common/inc/nx_api.h").exists())
        .unwrap_or(false)
}

/// `riscv64-unknown-elf-gcc` in PATH.
pub fn is_riscv_gcc_available() -> bool {
    Command::new("riscv64-unknown-elf-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `cmake` in PATH (for C / C++ examples).
#[allow(dead_code)]
pub fn is_cmake_available() -> bool {
    Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// =============================================================================
// Rust binary builders (cargo cross-compile to riscv64gc-unknown-none-elf)
// =============================================================================

static THREADX_RV64_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-riscv64-threadx/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX RISC-V example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!(
        "target/riscv64gc-unknown-none-elf/release/{}",
        binary_name
    ));
    super::require_prebuilt_binary(&binary_path)
}

pub fn build_threadx_rv64_talker() -> TestResult<&'static Path> {
    THREADX_RV64_TALKER_BINARY
        .get_or_try_init(|| build_rust_example("talker", "qemu-riscv64-threadx-talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_listener() -> TestResult<&'static Path> {
    THREADX_RV64_LISTENER_BINARY
        .get_or_try_init(|| build_rust_example("listener", "qemu-riscv64-threadx-listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_service_server() -> TestResult<&'static Path> {
    THREADX_RV64_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_rust_example("service-server", "qemu-riscv64-threadx-service-server")
        })
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_service_client() -> TestResult<&'static Path> {
    THREADX_RV64_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_rust_example("service-client", "qemu-riscv64-threadx-service-client")
        })
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_action_server() -> TestResult<&'static Path> {
    THREADX_RV64_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_rust_example("action-server", "qemu-riscv64-threadx-action-server")
        })
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_action_client() -> TestResult<&'static Path> {
    THREADX_RV64_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_rust_example("action-client", "qemu-riscv64-threadx-action-client")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// ThreadX RISC-V DDS variant (Phase 97.4.threadx-riscv64)
// =============================================================================

static THREADX_RV64_DDS_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static THREADX_RV64_DDS_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_dds_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!(
        "examples/qemu-riscv64-threadx/rust/dds/{}",
        name
    ));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX RISC-V DDS example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!(
        "target/riscv64gc-unknown-none-elf/release/{}",
        binary_name
    ));
    super::require_prebuilt_binary(&binary_path)
}

pub fn build_threadx_rv64_dds_talker() -> TestResult<&'static Path> {
    THREADX_RV64_DDS_TALKER_BINARY
        .get_or_try_init(|| build_dds_rust_example("talker", "qemu-riscv64-threadx-dds-talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_rv64_dds_listener() -> TestResult<&'static Path> {
    THREADX_RV64_DDS_LISTENER_BINARY
        .get_or_try_init(|| {
            build_dds_rust_example("listener", "qemu-riscv64-threadx-dds-listener")
        })
        .map(|p| p.as_path())
}

// =============================================================================
// C / C++ binary builders (CMake with RISC-V toolchain)
// =============================================================================

static RV64_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static RV64_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!(
        "examples/qemu-riscv64-threadx/{}/zenoh/{}",
        lang, name
    ));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build");
    let binary_path = build_dir.join(binary_name);
    super::require_prebuilt_binary(&binary_path)
}

pub fn build_rv64_c_talker() -> TestResult<&'static Path> {
    RV64_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "riscv64_threadx_c_talker"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_listener() -> TestResult<&'static Path> {
    RV64_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "riscv64_threadx_c_listener"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_service_server() -> TestResult<&'static Path> {
    RV64_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("c", "service-server", "riscv64_threadx_c_service_server")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_c_service_client() -> TestResult<&'static Path> {
    RV64_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("c", "service-client", "riscv64_threadx_c_service_client")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_c_action_server() -> TestResult<&'static Path> {
    RV64_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("c", "action-server", "riscv64_threadx_c_action_server")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_c_action_client() -> TestResult<&'static Path> {
    RV64_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("c", "action-client", "riscv64_threadx_c_action_client")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_talker() -> TestResult<&'static Path> {
    RV64_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "riscv64_threadx_cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_listener() -> TestResult<&'static Path> {
    RV64_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "riscv64_threadx_cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_service_server() -> TestResult<&'static Path> {
    RV64_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example(
                "cpp",
                "service-server",
                "riscv64_threadx_cpp_service_server",
            )
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_service_client() -> TestResult<&'static Path> {
    RV64_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example(
                "cpp",
                "service-client",
                "riscv64_threadx_cpp_service_client",
            )
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_action_server() -> TestResult<&'static Path> {
    RV64_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-server", "riscv64_threadx_cpp_action_server")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_action_client() -> TestResult<&'static Path> {
    RV64_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "action-client", "riscv64_threadx_cpp_action_client")
        })
        .map(|p| p.as_path())
}
