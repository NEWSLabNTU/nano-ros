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

    eprintln!("Building qemu-riscv64-threadx/rust/zenoh/{}...", name);

    // RISC-V board crate needs its own config dirs (not the Linux sim defaults).
    let rv_config = root.join("packages/boards/nros-threadx-qemu-riscv64/config");

    let output = duct::cmd!("cargo", "build", "--release")
        .dir(&example_dir)
        .env("THREADX_CONFIG_DIR", &rv_config)
        .env("NETX_CONFIG_DIR", &rv_config)
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
        "target/riscv64gc-unknown-none-elf/release/{}",
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

    eprintln!(
        "Building qemu-riscv64-threadx/{}/zenoh/{} (CMake)...",
        lang, name
    );

    let build_dir = example_dir.join("build");
    // Clean stale build to avoid cmake cache conflicts.
    let _ = std::fs::remove_dir_all(&build_dir);
    std::fs::create_dir_all(&build_dir).ok();

    let prefix_path = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let toolchain = format!(
        "-DCMAKE_TOOLCHAIN_FILE={}",
        root.join("cmake/toolchain/riscv64-threadx.cmake").display()
    );
    let threadx_dir = std::env::var("THREADX_DIR").unwrap_or_else(|_| {
        root.join("third-party/threadx/kernel")
            .display()
            .to_string()
    });
    let netx_dir = std::env::var("NETX_DIR").unwrap_or_else(|_| {
        root.join("third-party/threadx/netxduo")
            .display()
            .to_string()
    });
    let config_dir = root
        .join("packages/boards/nros-threadx-qemu-riscv64/config")
        .display()
        .to_string();
    let board_dir = root
        .join("packages/boards/nros-threadx-qemu-riscv64/c")
        .display()
        .to_string();
    let virtio_dir = root
        .join("packages/drivers/virtio-net-netx")
        .display()
        .to_string();

    let output = duct::cmd!(
        "cmake",
        "-S",
        &example_dir,
        "-B",
        &build_dir,
        &prefix_path,
        &toolchain,
        "-DNANO_ROS_PLATFORM=threadx_riscv64",
        &format!("-DTHREADX_DIR={threadx_dir}"),
        &format!("-DNETX_DIR={netx_dir}"),
        &format!("-DTHREADX_CONFIG_DIR={config_dir}"),
        &format!("-DTHREADX_BOARD_DIR={board_dir}"),
        &format!("-DVIRTIO_DRIVER_DIR={virtio_dir}"),
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

    let output = duct::cmd!("cmake", "--build", &build_dir, "--", "-j4")
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

    let binary_path = build_dir.join(binary_name);
    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
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
            build_cmake_example("cpp", "service-server", "riscv64_threadx_cpp_service_server")
        })
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_service_client() -> TestResult<&'static Path> {
    RV64_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-client", "riscv64_threadx_cpp_service_client")
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
