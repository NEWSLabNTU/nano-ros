//! ThreadX QEMU RISC-V 64-bit binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the ThreadX-RISC-V Rust / C /
//! C++ examples. Moved out of `tests/threadx_riscv64_qemu.rs` (Phase 85.5).

use crate::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

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

/// The riscv64 bare-metal gcc this host actually has — issue 0657.
///
/// `[board.qemu-riscv64-threadx]` provisions xPack's `riscv-none-elf-gcc`, so a
/// probe spelling only Ubuntu's `riscv64-unknown-elf-gcc` reported "no
/// toolchain" on a host that had one. Resolution order and the env override
/// live in `scripts/build/riscv64-toolchain.sh`; this is the test-side reader of
/// the same answer, kept to the candidate list rather than shelling out so a
/// unit test needs no shell.
fn riscv64_gcc() -> String {
    if let Ok(prefix) = std::env::var("NROS_RISCV64_PREFIX")
        && !prefix.is_empty()
    {
        return format!("{prefix}-gcc");
    }
    let store = std::env::var("NROS_SDK_STORE")
        .unwrap_or_else(|_| format!("{}/.nros/sdk", std::env::var("HOME").unwrap_or_default()));
    let dir = std::path::Path::new(&store).join("riscv-none-elf-gcc");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut versions: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        versions.sort();
        versions.reverse();
        for v in versions {
            let p = dir.join(v).join("bin/riscv-none-elf-gcc");
            if p.is_file() {
                return p.to_string_lossy().into_owned();
            }
        }
    }
    for cand in ["riscv-none-elf", "riscv64-unknown-elf", "riscv64-none-elf"] {
        let name = format!("{cand}-gcc");
        if std::env::var_os("PATH")
            .map(|paths| std::env::split_paths(&paths).any(|d| d.join(&name).is_file()))
            .unwrap_or(false)
        {
            return name;
        }
    }
    "riscv64-unknown-elf-gcc".to_string()
}

/// Whether a riscv64 bare-metal gcc is available at all.
pub fn is_riscv_gcc_available() -> bool {
    Command::new(riscv64_gcc())
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
    // issue 0556 — go through the manifest ROW, never a hand-spelled leaf path.
    //
    // This used to join `target-zenoh/<triple>/<profile>/<bin>` onto the example
    // dir. The row authors no `target_dir`, so its artifact root is
    // `<dir>/target`; `target-zenoh` matched no row, attribution failed, the
    // shared-group redirect never fired, and the resolver read a leaf tree the
    // fixture build had stopped writing MONTHS earlier:
    //
    //     Test fixture binary not prebuilt: examples/qemu-riscv64-threadx/rust/
    //       talker/target-zenoh/riscv64gc-unknown-none-elf/nros-relwithdebinfo/…
    //
    // while the build wrote `build/cargo-fixtures/threadx-riscv64-<slug>/…`. The
    // artifact on the authored path was from 06-13; both `rtos_e2e`
    // ThreadxRiscv64 cases read as failures for it, and looked like flaky QEMU.
    //
    // `build_threadx_rv64_rust_example_rmw` is the sibling that already does
    // this correctly for the same platform — one derivation, not two (#393:
    // move the test-side locator in the SAME commit as the build-side path).
    super::build_threadx_rv64_rust_example_rmw(name, binary_name, super::Rmw::Zenoh)
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
static RV64_C_ERRNO_ISOLATION_BINARY: OnceCell<PathBuf> = OnceCell::new();

static RV64_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static RV64_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve a ThreadX-RV64 cmake example's artifact for a NAMED rmw.
///
/// Issue 0786. The C and C++ pubsub tests used to hand-build
/// `examples/qemu-riscv64-threadx/<lang>/<case>/build-cyclonedds/<bin>` as a
/// plain `root.join(...)`, because the resolver below only ever spelled
/// `build-zenoh`. A hand-built path skips BOTH things this function exists to
/// do: the lane coordinate check, and `require_prebuilt_binary_fresh_cmake`.
///
/// The consequence was not a missing file — it was a five-day-old one. The
/// tier-2 build lane is 1-wise, so it need not rebuild this coordinate, and
/// the artifact from a previous lane sat there and RAN. A museum binary that
/// hangs looks exactly like a code regression: it cost a bisect-shaped
/// investigation and a filed issue before the mtimes were read.
///
/// So the rule the rest of the suite follows applies here too — resolve, never
/// join. The zenoh entry point below delegates here rather than keeping its own
/// copy of the path arithmetic, so the two cannot drift.
pub fn build_rv64_cmake_example_rmw(
    lang: &str,
    name: &str,
    binary_name: &str,
    rmw: super::Rmw,
) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-riscv64-threadx/{}/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join(rmw.build_dir());
    let binary_path = build_dir.join(binary_name);
    super::require_prebuilt_binary_fresh_cmake(&binary_path)
}

fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    build_rv64_cmake_example_rmw(lang, name, binary_name, super::Rmw::Zenoh)
}

pub fn build_rv64_c_talker() -> TestResult<&'static Path> {
    RV64_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "c_talker"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_listener() -> TestResult<&'static Path> {
    RV64_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "c_listener"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_service_server() -> TestResult<&'static Path> {
    RV64_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_service_client() -> TestResult<&'static Path> {
    RV64_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_action_server() -> TestResult<&'static Path> {
    RV64_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_rv64_c_action_client() -> TestResult<&'static Path> {
    RV64_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "c_action_client"))
        .map(|p| p.as_path())
}

/// issue 0680 — the per-thread-`errno` probe. Self-contained: one image, two
/// tasks, no peer and no messaging, so it is resolved like any other cmake
/// example but run alone.
pub fn build_rv64_c_errno_isolation() -> TestResult<&'static Path> {
    RV64_C_ERRNO_ISOLATION_BINARY
        .get_or_try_init(|| build_cmake_example("c", "errno-isolation", "c_errno_isolation"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_talker() -> TestResult<&'static Path> {
    RV64_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_listener() -> TestResult<&'static Path> {
    RV64_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_service_server() -> TestResult<&'static Path> {
    RV64_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-server", "cpp_service_server"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_service_client() -> TestResult<&'static Path> {
    RV64_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-client", "cpp_service_client"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_action_server() -> TestResult<&'static Path> {
    RV64_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-server", "cpp_action_server"))
        .map(|p| p.as_path())
}

pub fn build_rv64_cpp_action_client() -> TestResult<&'static Path> {
    RV64_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-client", "cpp_action_client"))
        .map(|p| p.as_path())
}
