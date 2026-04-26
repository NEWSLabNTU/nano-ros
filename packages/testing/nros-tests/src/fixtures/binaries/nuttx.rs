//! NuttX QEMU ARM virt binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the NuttX Rust / C / C++
//! examples. Moved out of `tests/nuttx_qemu.rs` (Phase 85.5).

use crate::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::path::{Path, PathBuf};
use std::process::Command;

// =============================================================================
// Prerequisite detection
// =============================================================================

/// `NUTTX_DIR` env var set and points to a valid kernel source tree.
pub fn is_nuttx_available() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("Makefile").exists())
        .unwrap_or(false)
}

/// NuttX has been configured — `$NUTTX_DIR/include/nuttx/config.h` exists.
pub fn is_nuttx_configured() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("include/nuttx/config.h").exists())
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

/// Trust that `just setup` installed the pinned NuttX nightly
/// toolchain. The pinned version lives in
/// `examples/qemu-arm-nuttx/rust-toolchain.toml` and is the
/// authoritative source — cargo auto-resolves it when invoked from
/// inside that directory tree. If setup wasn't run, the cargo build
/// will fail with an actionable "toolchain not found" message, which
/// is the correct behaviour per CLAUDE.md "fail on unmet preconditions".
pub fn is_nuttx_toolchain_available() -> bool {
    true
}

/// Path to a pre-built NuttX kernel image, if it exists.
pub fn nuttx_kernel_path() -> Option<PathBuf> {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("nuttx"))
        .filter(|p| p.exists())
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
// Rust binary builders (cargo nightly + -Z build-std)
// =============================================================================

static NUTTX_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/rust/zenoh/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!(
        "target/armv7a-nuttx-eabihf/release/{}",
        binary_name
    ));

    // Default contract: tests don't compile fixtures.
    if let Some(result) = super::require_prebuilt_binary(&binary_path) {
        return result;
    }

    eprintln!("Building qemu-arm-nuttx/rust/zenoh/{}...", name);

    // cc-rs doesn't recognize armv7a-nuttx-eabihf (Tier 3) and falls back to
    // the host `cc`, which fails on ARM flags. Set the target-specific CC so
    // cc-rs uses the ARM cross-compiler. Also unset RUSTUP_TOOLCHAIN so the
    // example's pinned nightly wins over the harness's stable default.
    let output = duct::cmd!("cargo", "build", "--release")
        .dir(&example_dir)
        .env("CC_armv7a_nuttx_eabi", "arm-none-eabi-gcc")
        .env_remove("RUSTUP_TOOLCHAIN")
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

pub fn build_nuttx_talker() -> TestResult<&'static Path> {
    NUTTX_TALKER_BINARY
        .get_or_try_init(|| build_rust_example("talker", "nuttx-rs-talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_listener() -> TestResult<&'static Path> {
    NUTTX_LISTENER_BINARY
        .get_or_try_init(|| build_rust_example("listener", "nuttx-rs-listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_service_server() -> TestResult<&'static Path> {
    NUTTX_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("service-server", "nuttx-rs-service-server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_service_client() -> TestResult<&'static Path> {
    NUTTX_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("service-client", "nuttx-rs-service-client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_action_server() -> TestResult<&'static Path> {
    NUTTX_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_rust_example("action-server", "nuttx-rs-action-server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_action_client() -> TestResult<&'static Path> {
    NUTTX_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_rust_example("action-client", "nuttx-rs-action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// C / C++ binary builders (CMake, via corrosion + nuttx_build_example)
// =============================================================================

static NUTTX_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static NUTTX_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/{}/zenoh/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX {lang} example not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build");
    let binary_path = build_dir.join(binary_name);

    // Default contract: tests don't compile fixtures.
    if let Some(result) = super::require_prebuilt_binary(&binary_path) {
        return result;
    }

    eprintln!("Building qemu-arm-nuttx/{}/zenoh/{} (CMake)...", lang, name);

    std::fs::create_dir_all(&build_dir).ok();

    let prefix_path = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let nuttx_dir = std::env::var("NUTTX_DIR")
        .unwrap_or_else(|_| root.join("third-party/nuttx/nuttx").display().to_string());

    // Corrosion invokes cargo internally. Unset RUSTUP_TOOLCHAIN so the
    // example tree's rust-toolchain.toml pinned nightly wins over stable
    // inherited from the nextest harness.
    let output = duct::cmd!(
        "cmake",
        "-S",
        &example_dir,
        "-B",
        &build_dir,
        &prefix_path,
        &format!("-DNUTTX_DIR={nuttx_dir}"),
        "-DCMAKE_BUILD_TYPE=Release"
    )
    .env_remove("RUSTUP_TOOLCHAIN")
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
        .env_remove("RUSTUP_TOOLCHAIN")
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
            "Binary not found: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

pub fn build_nuttx_cpp_talker() -> TestResult<&'static Path> {
    NUTTX_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "nuttx_cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_listener() -> TestResult<&'static Path> {
    NUTTX_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "nuttx_cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_service_server() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-server", "nuttx_cpp_service_server")
        })
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_service_client() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_cmake_example("cpp", "service-client", "nuttx_cpp_service_client")
        })
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_action_server() -> TestResult<&'static Path> {
    NUTTX_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-server", "nuttx_cpp_action_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_action_client() -> TestResult<&'static Path> {
    NUTTX_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-client", "nuttx_cpp_action_client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_talker() -> TestResult<&'static Path> {
    NUTTX_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "nuttx_c_talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_listener() -> TestResult<&'static Path> {
    NUTTX_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "nuttx_c_listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_service_server() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "nuttx_c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_service_client() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "nuttx_c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_action_server() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "nuttx_c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_action_client() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "nuttx_c_action_client"))
        .map(|p| p.as_path())
}
