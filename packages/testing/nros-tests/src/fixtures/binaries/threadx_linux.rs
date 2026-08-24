//! ThreadX Linux (native simulation) binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the ThreadX-Linux Rust / C /
//! C++ examples. Moved out of `tests/threadx_linux.rs` (Phase 85.5).

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

/// nsos-netx BSD shim source is available at the expected repo location.
pub fn is_nsos_netx_available() -> bool {
    let root = project_root();
    root.join("packages/drivers/net/nsos-netx/src/nsos_netx.c")
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

// Issue #194 — the role crates are lib-only Component pkgs since 212.L (same
// as FreeRTOS/NuttX, see freertos.rs `build_rust_example` / nuttx.rs
// `require_entry_binary`): the runnable image is the sibling `<role>-entry`
// Entry pkg the fixture rows prebuild. The old path probed a
// `target-zenoh/…/threadx-linux-<role>` bin the role crate can no longer
// produce — pre-212.L museum binaries (May 2026, pre-phase-277 output
// markers) satisfied the probe and delivered under the OLD `Received [N]:`
// marker, so the harness's `I heard:` grep counted 0 while delivery worked.
fn build_rust_example(name: &str, _binary_name: &str) -> TestResult<PathBuf> {
    // phase-338 W2 — the `-entry` package is collapsed into the role package,
    // and its `[[bin]]` took the short program name (`service-server`), the
    // same convention native uses.
    require_entry_binary(name, name)
}

/// Resolve a ThreadX-Linux cmake example's artifact for a NAMED rmw.
///
/// Issue 0786, sibling of the RV64 one. Four sites in `native_api.rs` built
/// `examples/threadx-linux/<lang>/<case>/build-cyclonedds/<bin>` by hand and
/// guarded it with `.exists()`. A museum binary EXISTS, so that guard passes
/// and the stale image runs — which is issue 0215 verbatim, recorded in a
/// comment at one of those very sites ("an orphaned museum binary in the
/// never-wiped build dir satisfied the existence check while silently broken").
/// The RV64 C++ cell then hit the same thing from the same cause and read as a
/// code regression.
///
/// `require_prebuilt_binary_fresh_cmake` answers the question `.exists()`
/// cannot: is this artifact NEWER than the sources it was built from.
pub fn build_threadx_cmake_example_rmw(
    lang: &str,
    name: &str,
    binary_name: &str,
    rmw: super::Rmw,
) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/threadx-linux/{}/{}", lang, name));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example not found: {}",
            example_dir.display()
        )));
    }
    let binary_path = example_dir.join(rmw.build_dir()).join(binary_name);
    super::require_prebuilt_binary_fresh_cmake(&binary_path)
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
// Phase 275 W1 (#102 H2) — per-role Entry-pkg demos (build-assert).
// =============================================================================

/// Resolve a prebuilt ThreadX-Linux `<role>_entry` host binary.
///
/// Prebuilt by the `[[fixture]]` rows in `examples/fixtures.toml` (built via
/// `fixtures-build.sh threadx-linux rust`, which runs `nros sync` + cargo).
/// `role` is hyphenated (`"service-server"`); `bin` is the `[[bin]]` name
/// (since phase-338 W2 collapsed the `-entry` package in, the same short
/// name: `"service-server"`).
///
/// The artifact is resolved through the GROUP resolver, never by hand. Two
/// things made the hand-built path wrong, and only one of them was visible:
///
///  * It spliced `x86_64-unknown-linux-gnu` in, because the Entry pkgs pinned
///    `[build] target` to that literal — a host pin on one machine and a cross
///    compile on every other (issue 0582). The pin is gone, so no triple.
///  * The prefix was wrong too, on every host. These rows author no
///    `target_dir`, so `fixtures-target-dir.sh` puts them in a shared group dir
///    (`build/cargo-fixtures/<group>/`) and cargo never writes `<leaf>/target`
///    at all. `fixtures-target-dir.sh` is explicit that the build and every
///    resolver MUST call the SAME resolver; this one did not.
///
/// The leaf carries TWO row-sets at one artifact root — these bare Entry rows
/// and the feature-selected role rows above — so `groups::attribute` reports
/// AMBIGUOUS and refuses to redirect (issue 0517, deliberately: guessing would
/// hand back a binary built with different features). `select_row` with the
/// row's variant is how a caller resolves that. These rows author `rmw` and
/// leave default features on, which is the `platform_rmw` shape.
pub fn require_entry_binary(role: &str, bin: &str) -> TestResult<PathBuf> {
    let leaf = format!("examples/threadx-linux/rust/{role}");
    let dir = project_root().join(&leaf);
    if !dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX Linux entry example not found: {}",
            dir.display()
        )));
    }
    let rel = PathBuf::from(format!("{}/{bin}", super::cargo_target_profile_dir()));
    if crate::fixtures::groups::leaf_has_rows(&leaf) {
        let row = crate::fixtures::groups::select_row(
            &leaf,
            &crate::fixtures::groups::FixtureVariant::platform_rmw(crate::fixtures::Rmw::Zenoh),
        )?;
        return super::require_prebuilt_row_binary_fresh(row, &rel);
    }
    super::require_prebuilt_binary_fresh(&dir.join("target").join(&rel))
}

// =============================================================================
// Phase 169.4b — ThreadX-Linux Rust DDS fixture builders deleted
// alongside the Rust DDS retirement (Phase 169.2 deleted the example
// crates).
// =============================================================================

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
    let example_dir = root.join(format!("examples/threadx-linux/{}/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ThreadX {lang} example directory not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build-zenoh");
    let binary_path = build_dir.join(binary_name);
    super::require_prebuilt_binary_fresh_cmake(&binary_path)
}

pub fn build_threadx_cpp_talker() -> TestResult<&'static Path> {
    THREADX_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_listener() -> TestResult<&'static Path> {
    THREADX_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_service_server() -> TestResult<&'static Path> {
    THREADX_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-server", "cpp_service_server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_cpp_service_client() -> TestResult<&'static Path> {
    THREADX_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-client", "cpp_service_client"))
        .map(|p| p.as_path())
}

#[allow(dead_code)]
pub fn build_threadx_cpp_action_server() -> TestResult<&'static Path> {
    THREADX_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-server", "cpp_action_server"))
        .map(|p| p.as_path())
}

#[allow(dead_code)]
pub fn build_threadx_cpp_action_client() -> TestResult<&'static Path> {
    THREADX_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-client", "cpp_action_client"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_talker() -> TestResult<&'static Path> {
    THREADX_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "c_talker"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_listener() -> TestResult<&'static Path> {
    THREADX_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "c_listener"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_service_server() -> TestResult<&'static Path> {
    THREADX_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_service_client() -> TestResult<&'static Path> {
    THREADX_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_action_server() -> TestResult<&'static Path> {
    THREADX_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_threadx_c_action_client() -> TestResult<&'static Path> {
    THREADX_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "c_action_client"))
        .map(|p| p.as_path())
}
