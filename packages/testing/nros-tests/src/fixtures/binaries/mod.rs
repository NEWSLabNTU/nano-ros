//! Binary build helpers for integration tests
//!
//! Provides functions to build test binaries with caching support.

pub mod freertos;
pub mod nuttx;
pub mod threadx_linux;
pub mod threadx_riscv64;

use crate::{TestError, TestResult, pinned_nightly, project_root};
use duct::cmd;
use once_cell::sync::OnceCell;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn workspace_fixture_stamp_name(fixture_id: &str) -> String {
    format!(".nros-workspace-fixture.{fixture_id}.inputsig")
}

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

/// Phase 115.F — cached path to the custom-transport-talker example.
static NATIVE_CT_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Phase 115.F — cached path to the custom-transport-listener example.
static NATIVE_CT_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Phase 211.I — cached path to the `tt-zenoh-to-xrce` bridge binary used by
/// the mixed-RMW bridge e2e (Phase 110.G.bridge example reused as fixture).
static NATIVE_BRIDGE_TT_ZENOH_XRCE_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-lifecycle-node binary
static NATIVE_LIFECYCLE_NODE_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-talker binary with safety-e2e
static NATIVE_TALKER_SAFETY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary with safety-e2e
static NATIVE_LISTENER_SAFETY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary with unstable-zenoh-api (zero-copy)
static NATIVE_LISTENER_ZERO_COPY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-talker binary with link-tls
static NATIVE_TALKER_TLS_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native-rs-listener binary with link-tls
static NATIVE_LISTENER_TLS_BINARY: OnceCell<PathBuf> = OnceCell::new();

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

// Phase 169.4 — bare-metal MPS2-AN385 DDS fixture statics removed
// (Phase 97.3.mps2-an385 lineage; deleted with the Rust DDS retirement).

/// Cached path to the qemu-serial-talker binary
static QEMU_SERIAL_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-serial-listener binary
static QEMU_SERIAL_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Phase 207 — cached path to the bare-metal XRCE talker binary.
static QEMU_TALKER_XRCE_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the esp32-qemu-talker binary (ELF)
static ESP32_QEMU_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the esp32-qemu-listener binary (ELF)
static ESP32_QEMU_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

// Phase 169.4b — ESP32-C3 QEMU DDS fixture statics removed alongside
// the Rust DDS retirement (Phase 169.2 deleted the example crates).

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

/// Cached path to the px4-stub binary (Phase 233.4 — PX4 XRCE companion).
static PX4_STUB_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the px4 offboard-companion binary (Phase 233.4).
static PX4_COMPANION_BINARY: OnceCell<PathBuf> = OnceCell::new();

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

/// Cached path to the native Rust workspace Entry pkg binary.
static NATIVE_WORKSPACE_RUST_ENTRY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native C workspace Entry pkg binary.
static NATIVE_WORKSPACE_C_ENTRY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native C++ workspace Entry pkg binary.
static NATIVE_WORKSPACE_CPP_ENTRY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native mixed C/C++ workspace Entry pkg binary.
static NATIVE_WORKSPACE_MIXED_ENTRY_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build the qemu-test example and return its path
///
/// Uses OnceLock to cache the build, so subsequent calls are fast.
pub fn build_qemu_test() -> TestResult<&'static Path> {
    QEMU_TEST_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("packages/testing/nros-tests/bins/cdr-roundtrip-qemu");

            eprintln!("Building qemu-test...");

            let mut args = cargo_build_args();
            args.push("--target".to_string());
            args.push("thumbv7m-none-eabi".to_string());

            let output = cmd("cargo", args)
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
                "target/thumbv7m-none-eabi/{}/qemu-rs-test",
                cargo_target_profile_dir()
            ));

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
/// Verify a test-fixture binary was prebuilt — the only contract.
/// Tests must not compile fixtures inside their bodies; the build phase
/// belongs to `just build-test-fixtures`, which sequences cargo/cmake/west
/// invocations cooperatively instead of letting them race with the host's
/// QEMU + zenohd test load. Builds inside test bodies historically
/// stretched a 14 s test to 125 s on a saturated host.
pub(crate) fn require_prebuilt_binary(binary_path: &Path) -> TestResult<PathBuf> {
    if binary_path.exists() {
        return Ok(binary_path.to_path_buf());
    }
    // Tier-aware (#25): the LIGHT host-integration lane (`NROS_FIXTURES_OPTIONAL=1`)
    // does not build every native fixture variant (TLS / cyclonedds / zero-copy /
    // workspace-entry need extra system deps + tools). There an unstaged fixture
    // is an environment-conditional skip, not a failure — `skip!` ([SKIPPED]) so
    // the [SKIPPED]-aware recipe treats it as a skip. The FULL `test-all` tier
    // leaves the var unset and still hard-fails, surfacing any real fixture gap.
    if std::env::var_os("NROS_FIXTURES_OPTIONAL").is_some() {
        crate::skip!(
            "fixture binary not prebuilt: {} (light tier; run `just build-test-fixtures` for full coverage)",
            binary_path.display()
        );
    }
    Err(TestError::BuildFailed(format!(
        "Test fixture binary not prebuilt: {}\n\
         Run `just build-test-fixtures` first.",
        binary_path.display()
    )))
}

fn cargo_profile_name() -> String {
    env::var("NROS_CARGO_PROFILE").unwrap_or_else(|_| "nros-fast-release".to_string())
}

fn cargo_target_profile_dir() -> String {
    match cargo_profile_name().as_str() {
        "dev" => "debug".to_string(),
        "release" => "release".to_string(),
        profile => profile.to_string(),
    }
}

fn cargo_build_args() -> Vec<String> {
    match cargo_profile_name().as_str() {
        "dev" => vec!["build".to_string()],
        "release" => vec!["build".to_string(), "--release".to_string()],
        profile => vec![
            "build".to_string(),
            "--profile".to_string(),
            profile.to_string(),
        ],
    }
}

pub fn build_example(
    name: &str,
    binary_name: &str,
    _features: Option<&[&str]>,
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

    let profile_dir = cargo_target_profile_dir();
    let binary_path = if let Some(target) = target {
        example_dir.join(format!("target/{}/{}/{}", target, profile_dir, binary_name))
    } else {
        example_dir.join(format!("target/{}/{}", profile_dir, binary_name))
    };

    require_prebuilt_binary(&binary_path)
}

/// Phase 118 — RMW selector for the per-feature collapsed example dirs.
///
/// Mirror of the per-feature `rmw-{zenoh,dds,xrce}` Cargo features
/// exposed by every `examples/<plat>/<lang>/<case>/Cargo.toml` after
/// the collapse. Build harness picks one feature + the matching
/// `--target-dir target-<rmw>/` so each RMW's incremental state stays
/// isolated from the others (same isolation pattern Phase 88 zero-copy
/// / safety-e2e use).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Rmw {
    Zenoh,
    Xrce,
    /// Phase 11W — Cyclone DDS. Today exercised by the Zephyr
    /// `prj-cyclonedds.conf` overlay path; native / FreeRTOS /
    /// ThreadX wiring follows once those platforms grow a
    /// cyclonedds backend. (Phase 171.A removed the dead `Rmw::Dds`
    /// dust-DDS variant — dust-DDS retired in Phase 169.)
    Cyclonedds,
}

impl Rmw {
    /// Cargo feature name (`rmw-zenoh` / `rmw-xrce` / `rmw-cyclonedds`).
    pub fn cargo_feature(self) -> &'static str {
        match self {
            Rmw::Zenoh => "rmw-zenoh",
            Rmw::Xrce => "rmw-xrce",
            Rmw::Cyclonedds => "rmw-cyclonedds",
        }
    }

    /// `--target-dir` suffix.
    pub fn target_dir(self) -> &'static str {
        match self {
            Rmw::Zenoh => "target-zenoh",
            Rmw::Xrce => "target-xrce",
            Rmw::Cyclonedds => "target-cyclonedds",
        }
    }

    /// `NROS_RMW` cmake cache value.
    pub fn cmake_value(self) -> &'static str {
        match self {
            Rmw::Zenoh => "zenoh",
            Rmw::Xrce => "xrce",
            Rmw::Cyclonedds => "cyclonedds",
        }
    }

    /// Per-RMW C / C++ build dir name. Same isolation pattern as
    /// `target_dir()` but for cmake.
    pub fn build_dir(self) -> &'static str {
        match self {
            Rmw::Zenoh => "build-zenoh",
            Rmw::Xrce => "build-xrce",
            Rmw::Cyclonedds => "build-cyclonedds",
        }
    }
}

/// Phase 118 — resolve a prebuilt binary for a collapsed-shape example
/// built under a specific RMW feature.
///
/// `name` is the example dir under `examples/` (e.g. `"native/rust/talker"`,
/// without a `<rmw>` axis). `binary_name` is the Cargo `[[bin]] name`.
/// The build is expected to live at
/// `examples/<name>/<rmw.target_dir()>/<profile>/<binary_name>` — the
/// harness asserts the binary exists, mirroring `require_prebuilt_binary`'s
/// contract. The actual `cargo build --no-default-features --features <rmw>
/// --target-dir <rmw.target_dir()>` invocation belongs to
/// `just <plat> build-fixtures`.
pub fn build_example_rmw(name: &str, binary_name: &str, rmw: Rmw) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!(
        "{}/{}/{}",
        rmw.target_dir(),
        cargo_target_profile_dir(),
        binary_name
    ));
    require_prebuilt_binary(&binary_path)
}

/// Phase 118 — resolve a prebuilt binary for a collapsed-shape C / C++
/// example built under a specific RMW (cmake `-DNROS_RMW=<rmw>`).
///
/// `name` is the example dir under `examples/` (e.g. `"native/c/talker"`).
/// `binary_name` is the cmake `add_executable` target name. The build
/// is expected to land at
/// `examples/<name>/<rmw.build_dir()>/<binary_name>`. The actual
/// `cmake -B build-<rmw> -S . -DNROS_RMW=<rmw> && cmake --build
/// build-<rmw>` invocation belongs to `just <plat> build-fixtures`.
pub fn build_example_cmake_rmw(name: &str, binary_name: &str, rmw: Rmw) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }

    let binary_path = example_dir.join(format!("{}/{}", rmw.build_dir(), binary_name));
    require_prebuilt_binary(&binary_path)
}

fn workspace_example_dir(name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/workspaces/{name}"));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Workspace example directory not found: {}",
            example_dir.display()
        )));
    }
    Ok(example_dir)
}

fn current_workspace_fixture_record(fixture_id: &str) -> TestResult<String> {
    let root = project_root();
    let output = Command::new("python3")
        .arg(root.join("scripts/build/fixtures-manifest.py"))
        .arg("list-workspaces")
        .arg("--platform")
        .arg("native")
        .current_dir(&root)
        .output()
        .map_err(|e| {
            TestError::BuildFailed(format!("Failed to run workspace fixture manifest: {e}"))
        })?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "Failed to read workspace fixture manifest:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let prefix = format!("{fixture_id}\x1f");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.starts_with(&prefix))
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            TestError::BuildFailed(format!(
                "Workspace fixture {fixture_id:?} is not declared in examples/fixtures.toml"
            ))
        })
}

fn current_workspace_fixture_signature(fixture_id: &str) -> TestResult<String> {
    let root = project_root();
    let record = current_workspace_fixture_record(fixture_id)?;
    let output = Command::new("bash")
        .arg(root.join("scripts/build/workspace-fixture-signature.sh"))
        .arg(&record)
        .current_dir(&root)
        .output()
        .map_err(|e| {
            TestError::BuildFailed(format!("Failed to run workspace fixture signature: {e}"))
        })?;

    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "Failed to compute workspace fixture signature:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

fn require_prebuilt_workspace_binary(
    fixture_id: &str,
    binary_path: &Path,
    stamp_path: &Path,
) -> TestResult<PathBuf> {
    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Workspace fixture binary not prebuilt: {}\n\
             Run `just native build-workspace-fixtures` first.",
            binary_path.display()
        )));
    }

    let expected = current_workspace_fixture_signature(fixture_id)?;
    let actual = fs::read_to_string(stamp_path).map_err(|e| {
        TestError::BuildFailed(format!(
            "Workspace fixture stamp missing for {fixture_id}: {} ({e})\n\
             Run `just native build-workspace-fixtures` first.",
            stamp_path.display()
        ))
    })?;
    if actual.trim_end() != expected {
        return Err(TestError::BuildFailed(format!(
            "Workspace fixture {fixture_id} is stale: {}\n\
             Run `just native build-workspace-fixtures` first.",
            stamp_path.display()
        )));
    }

    Ok(binary_path.to_path_buf())
}

/// Resolve a prebuilt Rust workspace Entry pkg binary.
///
/// The workspace fixture build step owns `nros ws sync`,
/// `nros codegen-system`, and the Cargo build. Tests only require the
/// resulting binary from the deterministic fixture target dir.
pub fn build_workspace_rust_entry(
    fixture_id: &str,
    workspace: &str,
    binary_name: &str,
) -> TestResult<PathBuf> {
    let example_dir = workspace_example_dir(workspace)?;
    let target_dir = example_dir.join("target-fixtures");
    let binary_path = example_dir.join(format!(
        "target-fixtures/{}/{}",
        cargo_target_profile_dir(),
        binary_name
    ));
    require_prebuilt_workspace_binary(
        fixture_id,
        &binary_path,
        &target_dir.join(workspace_fixture_stamp_name(fixture_id)),
    )
}

/// Resolve a prebuilt CMake workspace Entry pkg binary.
///
/// The workspace fixture build step owns `nros ws sync`,
/// `nros codegen-system`, and the CMake configure/build. Tests only
/// require the resulting binary from the deterministic fixture build dir.
pub fn build_workspace_cmake_entry(
    fixture_id: &str,
    workspace: &str,
    binary_name: &str,
) -> TestResult<PathBuf> {
    let example_dir = workspace_example_dir(workspace)?;
    let build_dir = example_dir.join("build-workspace-fixtures");
    let binary_path = example_dir.join(format!(
        "build-workspace-fixtures/src/{binary_name}/{binary_name}"
    ));
    require_prebuilt_workspace_binary(
        fixture_id,
        &binary_path,
        &build_dir.join(workspace_fixture_stamp_name(fixture_id)),
    )
}

/// Native Rust workspace Entry pkg fixture.
pub fn build_native_workspace_rust_entry() -> TestResult<&'static Path> {
    NATIVE_WORKSPACE_RUST_ENTRY_BINARY
        .get_or_try_init(|| {
            build_workspace_rust_entry("workspace-rust-native", "rust", "native_entry")
        })
        .map(|p| p.as_path())
}

/// Native C workspace Entry pkg fixture.
pub fn build_native_workspace_c_entry() -> TestResult<&'static Path> {
    NATIVE_WORKSPACE_C_ENTRY_BINARY
        .get_or_try_init(|| build_workspace_cmake_entry("workspace-c-native", "c", "native_entry"))
        .map(|p| p.as_path())
}

/// Native C++ workspace Entry pkg fixture.
pub fn build_native_workspace_cpp_entry() -> TestResult<&'static Path> {
    NATIVE_WORKSPACE_CPP_ENTRY_BINARY
        .get_or_try_init(|| {
            build_workspace_cmake_entry("workspace-cpp-native", "cpp", "native_entry")
        })
        .map(|p| p.as_path())
}

/// Native mixed C/C++ workspace Entry pkg fixture.
pub fn build_native_workspace_mixed_entry() -> TestResult<&'static Path> {
    NATIVE_WORKSPACE_MIXED_ENTRY_BINARY
        .get_or_try_init(|| {
            build_workspace_cmake_entry("workspace-mixed-native", "mixed", "native_entry")
        })
        .map(|p| p.as_path())
}

/// Phase 118 — collapsed-shape native C talker, RMW-parametrized.
///
/// Returns the prebuilt binary for the named RMW. The fixture build
/// chain (`just native build-fixtures`) configures + builds
/// `examples/native/c/talker/` once per RMW into separate
/// `build-{zenoh,dds,xrce}/` dirs.
pub fn build_native_c_talker_rmw(rmw: Rmw) -> TestResult<&'static Path> {
    static ZENOH_CELL: OnceCell<PathBuf> = OnceCell::new();
    static XRCE_CELL: OnceCell<PathBuf> = OnceCell::new();
    static CYCLONEDDS_CELL: OnceCell<PathBuf> = OnceCell::new();
    let cell = match rmw {
        Rmw::Zenoh => &ZENOH_CELL,
        Rmw::Xrce => &XRCE_CELL,
        Rmw::Cyclonedds => &CYCLONEDDS_CELL,
    };
    cell.get_or_try_init(|| build_example_cmake_rmw("native/c/talker", "c_talker", rmw))
        .map(|p| p.as_path())
}

/// Phase 131.B — resolve a prebuilt test-fixture / bench binary that lives
/// under `packages/testing/nros-{tests/bins,bench,smoke}/<crate>/`.
///
/// `crate_subpath` is the path *under* `packages/testing/` (e.g.
/// `"nros-tests/bins/cdr-roundtrip-qemu"`).
pub fn build_test_fixture(
    crate_subpath: &str,
    binary_name: &str,
    target: Option<&str>,
) -> TestResult<PathBuf> {
    let root = project_root();
    let crate_dir = root.join(format!("packages/testing/{}", crate_subpath));

    if !crate_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Test fixture crate directory not found: {}",
            crate_dir.display()
        )));
    }

    let profile_dir = cargo_target_profile_dir();
    let binary_path = if let Some(target) = target {
        crate_dir.join(format!("target/{}/{}/{}", target, profile_dir, binary_name))
    } else {
        crate_dir.join(format!("target/{}/{}", profile_dir, binary_name))
    };

    require_prebuilt_binary(&binary_path)
}

/// Phase 226.D — migrated platforms whose default-config standalone Rust
/// fixtures share one fixture-only Cargo target dir
/// (`build/fixtures-cargo/<platform>`). This mirrors the shell resolver in
/// `scripts/build/fixtures-target-dir.sh`: `scripts/build/fixtures-build.sh`
/// builds eligible rows into the shared dir, so the test harness must
/// resolve the prebuilt binary there instead of the example-local
/// `target/`. ESP32 flash packaging and RTOS rows are deferred to a later
/// patch (they carry extra postprocessing).
///
/// Returns `None` for platforms not yet migrated, so unrelated callers
/// keep their example-local resolution. Only the *default* group (no
/// extra features/env) is mirrored here, matching the only rows these two
/// platforms carry today; a future feature/env variant would get a
/// hashed group slug on the shell side and would need an explicit mirror.
fn fixture_shared_target_dir(platform: &str) -> Option<PathBuf> {
    match platform {
        "qemu-arm-baremetal" | "stm32f4" => {
            Some(project_root().join("build/fixtures-cargo").join(platform))
        }
        _ => None,
    }
}

/// Phase 226.D — resolve a prebuilt standalone Rust fixture that builds
/// into the shared fixture target dir. `platform` selects the group,
/// `triple` is the cross target, `binary_name` is the Cargo `[[bin]]`
/// name. The binary lands at
/// `build/fixtures-cargo/<platform>/<triple>/<profile>/<binary_name>`.
fn require_shared_fixture_binary(
    platform: &str,
    triple: &str,
    binary_name: &str,
) -> TestResult<PathBuf> {
    let target_dir = fixture_shared_target_dir(platform).ok_or_else(|| {
        TestError::BuildFailed(format!(
            "Phase 226.D: platform {platform:?} is not migrated to a shared fixture target dir"
        ))
    })?;
    let binary_path = target_dir.join(format!(
        "{triple}/{}/{}",
        cargo_target_profile_dir(),
        binary_name
    ));
    require_prebuilt_binary(&binary_path)
}

/// Phase 226.D — qemu-arm-baremetal (`thumbv7m-none-eabi`) shared-fixture
/// binary resolver.
fn require_qemu_baremetal_fixture(binary_name: &str) -> TestResult<PathBuf> {
    require_shared_fixture_binary("qemu-arm-baremetal", "thumbv7m-none-eabi", binary_name)
}

/// Phase 226.D — stm32f4 (`thumbv7em-none-eabihf`) shared-fixture binary
/// resolver.
fn require_stm32f4_fixture(binary_name: &str) -> TestResult<PathBuf> {
    require_shared_fixture_binary("stm32f4", "thumbv7em-none-eabihf", binary_name)
}

/// Build native-rs-talker with param-services feature (cached)
pub fn build_native_talker() -> TestResult<&'static Path> {
    NATIVE_TALKER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/talker",
                "talker",
                Some(&["param-services"]),
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Phase 118 — collapsed-shape native talker, RMW-parametrized.
///
/// Returns the prebuilt binary for the named RMW. Phase 220.C path B
/// retired the cmake/corrosion cyclonedds bridge; every RMW (incl.
/// Cyclone) now resolves to a pure-cargo `target-<rmw>/<profile>/talker`
/// binary produced by `just native build-fixtures`. Cached per RMW so
/// repeated lookups in a nextest run avoid filesystem-stat overhead.
pub fn build_native_talker_rmw(rmw: Rmw) -> TestResult<&'static Path> {
    static ZENOH_CELL: OnceCell<PathBuf> = OnceCell::new();
    static XRCE_CELL: OnceCell<PathBuf> = OnceCell::new();
    static CYCLONEDDS_CELL: OnceCell<PathBuf> = OnceCell::new();
    let cell = match rmw {
        Rmw::Zenoh => &ZENOH_CELL,
        Rmw::Xrce => &XRCE_CELL,
        Rmw::Cyclonedds => &CYCLONEDDS_CELL,
    };
    cell.get_or_try_init(|| build_example_rmw("native/rust/talker", "talker", rmw))
        .map(|p| p.as_path())
}

/// Phase 118 — collapsed-shape native listener, RMW-parametrized.
///
/// See `build_native_talker_rmw` — same pure-cargo path post-220.C.
pub fn build_native_listener_rmw(rmw: Rmw) -> TestResult<&'static Path> {
    static ZENOH_CELL: OnceCell<PathBuf> = OnceCell::new();
    static XRCE_CELL: OnceCell<PathBuf> = OnceCell::new();
    static CYCLONEDDS_CELL: OnceCell<PathBuf> = OnceCell::new();
    let cell = match rmw {
        Rmw::Zenoh => &ZENOH_CELL,
        Rmw::Xrce => &XRCE_CELL,
        Rmw::Cyclonedds => &CYCLONEDDS_CELL,
    };
    cell.get_or_try_init(|| build_example_rmw("native/rust/listener", "listener", rmw))
        .map(|p| p.as_path())
}

/// Phase 118 — generic native Rust example resolver. Cuts repetition
/// when the test only needs a single (case, rmw) tuple instead of the
/// pre-cached talker/listener wrappers.
pub fn build_native_rust_example_rmw(
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_rmw(&format!("native/rust/{}", case), binary_name, rmw)
}

/// Phase 118 — generic native C example resolver. `case` is the
/// directory name under `examples/native/c/` (talker, listener,
/// service-server, …); `binary_name` is the cmake target (e.g.
/// `c_talker`, `c_service_server`, …).
pub fn build_native_c_example_rmw(case: &str, binary_name: &str, rmw: Rmw) -> TestResult<PathBuf> {
    build_example_cmake_rmw(&format!("native/c/{}", case), binary_name, rmw)
}

/// Phase 118 — generic native C++ example resolver. Mirror of the C
/// helper for `examples/native/cpp/<case>/`.
pub fn build_native_cpp_example_rmw(
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_cmake_rmw(&format!("native/cpp/{}", case), binary_name, rmw)
}

/// Phase 118.C — collapsed-shape ThreadX-RV64 Rust example resolver.
/// Zenoh uses the pure-cargo target dir; CycloneDDS uses the
/// CMake/Corrosion staticlib path added in Phase 175.B.
pub fn build_threadx_rv64_rust_example_rmw(
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-riscv64-threadx/rust/{}", case));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }
    let binary_path = if rmw == Rmw::Cyclonedds {
        example_dir.join(format!("{}/{}", rmw.build_dir(), binary_name))
    } else {
        example_dir.join(format!(
            "{}/riscv64gc-unknown-none-elf/{}/{}",
            rmw.target_dir(),
            cargo_target_profile_dir(),
            binary_name
        ))
    };
    require_prebuilt_binary(&binary_path)
}

/// Phase 118.B.7 — collapsed-shape threadx-linux Rust example resolver.
pub fn build_threadx_linux_rust_example_rmw(
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_rmw(&format!("threadx-linux/rust/{}", case), binary_name, rmw)
}

/// Phase 118.B.7 — collapsed-shape threadx-linux C / C++ example resolver.
pub fn build_threadx_linux_cmake_example_rmw(
    lang: &str,
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_cmake_rmw(
        &format!("threadx-linux/{}/{}", lang, case),
        binary_name,
        rmw,
    )
}

/// Phase 168.1 — collapsed-shape Zephyr Rust example resolver.
///
/// Zephyr west builds drop the artifact at
/// `zephyr-workspace/build-rs-<case>-<rmw>/zephyr/zephyr.exe` (not
/// inside the example dir), so this helper resolves to that path
/// instead of using `build_example_rmw`. `case` is the directory
/// name under `examples/zephyr/rust/` (talker, listener, …).
fn zephyr_build_root() -> PathBuf {
    if let Some(path) = std::env::var_os("NROS_ZEPHYR_BUILD_ROOT") {
        return PathBuf::from(path);
    }
    let root = project_root();
    // Mirror just/zephyr.just's ZEPHYR_WORKSPACE selection: the in-tree
    // `zephyr-workspace` (canonical), else the legacy `../nano-ros-workspace`
    // sibling. The build stages fixtures into whichever it picks (when
    // writable), falling back to `build/zephyr-workspace-builds` only when no
    // writable workspace exists — so the resolver must look in the same order.
    let in_tree = root.join("zephyr-workspace");
    let workspace = if in_tree.is_dir() || in_tree.is_symlink() {
        in_tree
    } else {
        match root.parent().map(|p| p.join("nano-ros-workspace")) {
            Some(sibling) if sibling.is_dir() => sibling,
            _ => in_tree,
        }
    };
    if workspace
        .metadata()
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
    {
        workspace
    } else {
        root.join("build/zephyr-workspace-builds")
    }
}

/// Build orchestration lives in `just/zephyr.just :: build-fixtures`.
pub fn build_zephyr_rust_example_rmw(case: &str, rmw: Rmw) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/zephyr/rust/{}", case));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }
    let binary_path = zephyr_build_root().join(format!(
        "build-rs-{}-{}/zephyr/zephyr.exe",
        case,
        rmw.cmake_value()
    ));
    require_prebuilt_binary(&binary_path)
}

/// Phase 168.4 — collapsed-shape Zephyr C / C++ example resolver.
/// `lang` is `"c"` or `"cpp"`. Mirrors the Rust resolver.
pub fn build_zephyr_cmake_example_rmw(lang: &str, case: &str, rmw: Rmw) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/zephyr/{}/{}", lang, case));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }
    let binary_path = zephyr_build_root().join(format!(
        "build-{}-{}-{}/zephyr/zephyr.exe",
        lang,
        case,
        rmw.cmake_value()
    ));
    require_prebuilt_binary(&binary_path)
}

/// Phase 118.C — collapsed-shape ThreadX-RV64 C / C++ example resolver.
pub fn build_threadx_rv64_cmake_example_rmw(
    lang: &str,
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_cmake_rmw(
        &format!("qemu-riscv64-threadx/{}/{}", lang, case),
        binary_name,
        rmw,
    )
}

/// Phase 118.B.5 — collapsed-shape NuttX C / C++ example resolver.
pub fn build_nuttx_cmake_example_rmw(
    lang: &str,
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_cmake_rmw(
        &format!("qemu-arm-nuttx/{}/{}", lang, case),
        binary_name,
        rmw,
    )
}

/// Phase 118.D — collapsed-shape FreeRTOS C / C++ example resolver.
/// `lang` is `"c"` or `"cpp"`. Binary lands at
/// `examples/qemu-arm-freertos/<lang>/<case>/build-<rmw>/<binary>`.
pub fn build_freertos_cmake_example_rmw(
    lang: &str,
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    build_example_cmake_rmw(
        &format!("qemu-arm-freertos/{}/{}", lang, case),
        binary_name,
        rmw,
    )
}

/// Phase 118.D — collapsed-shape FreeRTOS Rust example resolver.
///
/// FreeRTOS zenoh/xrce Rust examples are cross-compiled to
/// `target-<rmw>/thumbv7m-none-eabi/<profile>/<binary>`.
///
/// Phase 220.C path B — the CycloneDDS Rust fixture is retired from the
/// cmake/corrosion bridge (`build-cyclonedds/`); a pure-cargo FreeRTOS
/// cyclonedds path is deferred behind Phase 214.S.5.b's BSP gate
/// (cyclonedds-sys vendored build against the ARM cross toolchain +
/// FreeRTOS POSIX shim). Until that lands the cyclonedds branch returns
/// a `BuildFailed` error so callers (`freertos_qemu.rs`) emit the
/// proper `nros_tests::skip!` rather than silently passing.
pub fn build_freertos_rust_example_rmw(
    case: &str,
    binary_name: &str,
    rmw: Rmw,
) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-freertos/rust/{}", case));
    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example directory not found: {}",
            example_dir.display()
        )));
    }
    if rmw == Rmw::Cyclonedds {
        return Err(TestError::BuildFailed(format!(
            "Phase 220.C path B: FreeRTOS rust cyclonedds fixture retired \
             (cmake-bridge removed; pure-cargo path blocked on Phase \
             214.S.5.b BSP gate). Requested: {}/{}",
            case, binary_name
        )));
    }
    let binary_path = example_dir.join(format!(
        "{}/thumbv7m-none-eabi/{}/{}",
        rmw.target_dir(),
        cargo_target_profile_dir(),
        binary_name
    ));
    require_prebuilt_binary(&binary_path)
}

/// Build native-rs-listener (cached)
pub fn build_native_listener() -> TestResult<&'static Path> {
    NATIVE_LISTENER_BINARY
        .get_or_try_init(|| build_example("native/rust/listener", "listener", None, None))
        .map(|p| p.as_path())
}

/// Phase 115.F — build the custom-transport talker example (cached).
pub fn build_native_custom_transport_talker() -> TestResult<&'static Path> {
    NATIVE_CT_TALKER_BINARY
        .get_or_try_init(|| {
            build_example("native/rust/custom-transport-talker", "talker", None, None)
        })
        .map(|p| p.as_path())
}

/// Phase 211.I — resolve the prebuilt mixed-RMW bridge fixture binary
/// (`packages/testing/nros-tests/bins/bridge-zenoh-to-xrce-fwd`). Used by
/// `tests/bridge_mixed_rmw.rs` to forward zenoh `/chatter` samples into an
/// XRCE-DDS session. A minimal sibling to the Phase 110.G
/// `tt-zenoh-to-xrce` example: same dual-session topology, but the type
/// name matches `std_msgs::msg::Int32` (the type the talker/listener
/// fixtures use) and no TT-window gating — the 211.I assertion is "a
/// sample crosses the RMW boundary", which the TT example's String-type
/// constants would block at keyexpr registration.
///
/// The fixture sits in its own Cargo workspace (`[workspace]` table); the
/// test skips cleanly when the binary is missing.
pub fn build_bridge_zenoh_to_xrce_fwd() -> TestResult<&'static Path> {
    NATIVE_BRIDGE_TT_ZENOH_XRCE_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let dir = root.join("packages/testing/nros-tests/bins/bridge-zenoh-to-xrce-fwd");
            let profile = cargo_target_profile_dir();
            let binary = dir.join(format!("target/{profile}/bridge-zenoh-to-xrce-fwd"));
            require_prebuilt_binary(&binary)
        })
        .map(|p| p.as_path())
}

/// Phase 115.F — build the custom-transport listener example (cached).
pub fn build_native_custom_transport_listener() -> TestResult<&'static Path> {
    NATIVE_CT_LISTENER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/custom-transport-listener",
                "listener",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native-rs-lifecycle-node (cached)
///
/// Enables `lifecycle-services` so the `ros2 lifecycle *` service surface
/// is exposed for interop tests.
pub fn build_native_lifecycle_node() -> TestResult<&'static Path> {
    NATIVE_LIFECYCLE_NODE_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/lifecycle-node",
                "lifecycle-node",
                Some(&["lifecycle-services"]),
                None,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-test binary path
#[rstest::fixture]
pub fn qemu_binary() -> PathBuf {
    build_qemu_test()
        .expect("Failed to build qemu-test")
        .to_path_buf()
}

/// Cached path to the Phase 88.15.a `logging-smoke-mps2-baremetal`
/// fixture binary (bare-metal MPS2-AN385 nros-log smoke).
static LOGGING_SMOKE_MPS2_BAREMETAL_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.a logging smoke binary. The
/// fixture must already be built (`just qemu build-fixtures`).
pub fn build_logging_smoke_mps2_baremetal() -> TestResult<&'static Path> {
    LOGGING_SMOKE_MPS2_BAREMETAL_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("logging-smoke-mps2-baremetal"))
        .map(|p| p.as_path())
}

/// Cached path to the Phase 88.15.b `logging-smoke-freertos-mps2`
/// fixture binary (MPS2-AN385 + FreeRTOS + lwIP nros-log smoke).
static LOGGING_SMOKE_FREERTOS_MPS2_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.b logging smoke binary.
pub fn build_logging_smoke_freertos_mps2() -> TestResult<&'static Path> {
    LOGGING_SMOKE_FREERTOS_MPS2_BINARY
        .get_or_try_init(|| {
            build_test_fixture(
                "nros-tests/bins/logging-smoke-freertos-mps2",
                "logging-smoke-freertos-mps2",
                Some("thumbv7m-none-eabi"),
            )
        })
        .map(|p| p.as_path())
}

/// Cached path to the Phase 88.15.d `logging-smoke-threadx-riscv64`
/// fixture binary (ThreadX + NetX Duo on QEMU `virt` RV64).
static LOGGING_SMOKE_THREADX_RISCV64_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.d logging smoke binary.
pub fn build_logging_smoke_threadx_riscv64() -> TestResult<&'static Path> {
    LOGGING_SMOKE_THREADX_RISCV64_BINARY
        .get_or_try_init(|| {
            build_test_fixture(
                "nros-tests/bins/logging-smoke-threadx-riscv64",
                "logging-smoke-threadx-riscv64",
                Some("riscv64gc-unknown-none-elf"),
            )
        })
        .map(|p| p.as_path())
}

/// Cached path to the `logging-smoke-threadx-linux` fixture binary.
static LOGGING_SMOKE_THREADX_LINUX_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt ThreadX Linux logging smoke binary.
pub fn build_logging_smoke_threadx_linux() -> TestResult<&'static Path> {
    LOGGING_SMOKE_THREADX_LINUX_BINARY
        .get_or_try_init(|| {
            build_test_fixture(
                "nros-tests/bins/logging-smoke-threadx-linux",
                "logging-smoke-threadx-linux",
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Cached path to the Phase 88.15.f `logging-smoke-esp32-qemu`
/// flash image (ESP32-C3 binary under stock `qemu-system-riscv32 -M
/// esp32c3`).
static LOGGING_SMOKE_ESP32_QEMU_FLASH: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.f logging smoke flash image.
/// Built by `just esp32 build-logging-smoke` (or whichever recipe
/// invokes the espflash `save-image` step against the fixture's
/// ELF output).
pub fn build_logging_smoke_esp32_qemu_flash() -> TestResult<&'static Path> {
    LOGGING_SMOKE_ESP32_QEMU_FLASH
        .get_or_try_init(|| {
            build_test_fixture(
                "nros-tests/bins/logging-smoke-esp32-qemu",
                "logging-smoke-esp32-qemu.bin",
                Some("riscv32imc-unknown-none-elf"),
            )
        })
        .map(|p| p.as_path())
}

/// Cached path to the Phase 88.15.c `logging-smoke-nuttx-qemu-arm`
/// fixture binary (NuttX flat-build kernel image for QEMU ARM virt).
static LOGGING_SMOKE_NUTTX_QEMU_ARM_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.c logging smoke binary. Built
/// by `just nuttx build-fixtures` (folded the fixture into the same
/// parallel sweep that builds the NuttX example tree).
pub fn build_logging_smoke_nuttx_qemu_arm() -> TestResult<&'static Path> {
    LOGGING_SMOKE_NUTTX_QEMU_ARM_BINARY
        .get_or_try_init(|| {
            build_test_fixture(
                "nros-tests/bins/logging-smoke-nuttx-qemu-arm",
                "logging-smoke-nuttx-qemu-arm",
                Some("armv7a-nuttx-eabihf"),
            )
        })
        .map(|p| p.as_path())
}

/// Cached path to the Phase 88.15.e `logging-smoke-zephyr-native-sim`
/// fixture binary (Zephyr `native_sim/native/64` running as a Linux
/// process).
static LOGGING_SMOKE_ZEPHYR_NATIVE_SIM_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt Phase 88.15.e logging smoke binary. Built
/// by `just zephyr build-logging-smoke` (or whichever recipe wires
/// the fixture into `just zephyr build-fixtures`). The Zephyr
/// `native_sim` flow emits a Linux ELF under
/// `<zephyr-workspace>/build-logging-smoke/zephyr/zephyr.exe`.
pub fn build_logging_smoke_zephyr_native_sim() -> TestResult<&'static Path> {
    LOGGING_SMOKE_ZEPHYR_NATIVE_SIM_BINARY
        .get_or_try_init(|| {
            let binary = zephyr_build_root().join("build-logging-smoke/zephyr/zephyr.exe");
            require_prebuilt_binary(&binary)
        })
        .map(|p| p.as_path())
}

/// Build the qemu-wcet-bench example and return its path (cached)
pub fn build_qemu_wcet_bench() -> TestResult<&'static Path> {
    QEMU_WCET_BENCH_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rs-wcet-bench"))
        .map(|p| p.as_path())
}

/// Build the qemu-lan9118 example and return its path (cached)
pub fn build_qemu_lan9118() -> TestResult<&'static Path> {
    QEMU_LAN9118_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rs-lan9118"))
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

/// rstest fixture that provides the native-rs-lifecycle-node binary path
#[rstest::fixture]
pub fn lifecycle_node_binary() -> PathBuf {
    build_native_lifecycle_node()
        .expect("Failed to build native-rs-lifecycle-node")
        .to_path_buf()
}

/// Build native-rs-talker with link-tls feature (cached)
///
/// Uses a separate `target-tls` directory to avoid overwriting the
/// standard talker binary that other parallel test processes use.
pub fn build_native_talker_tls() -> TestResult<&'static Path> {
    NATIVE_TALKER_TLS_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/talker");
            let target_dir = example_dir.join("target-tls");
            let binary_path = target_dir.join(format!("{}/talker", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
        })
        .map(|p| p.as_path())
}

/// Build native-rs-listener with link-tls feature (cached)
///
/// Uses a separate `target-tls` directory to avoid overwriting the
/// standard listener binary that other parallel test processes use.
pub fn build_native_listener_tls() -> TestResult<&'static Path> {
    NATIVE_LISTENER_TLS_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("examples/native/rust/listener");
            let target_dir = example_dir.join("target-tls");
            let binary_path = target_dir.join(format!("{}/listener", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the native-rs-talker binary path (with link-tls)
#[rstest::fixture]
pub fn talker_tls_binary() -> PathBuf {
    build_native_talker_tls()
        .expect("Failed to build native-rs-talker with link-tls")
        .to_path_buf()
}

/// rstest fixture that provides the native-rs-listener binary path (with link-tls)
#[rstest::fixture]
pub fn listener_tls_binary() -> PathBuf {
    build_native_listener_tls()
        .expect("Failed to build native-rs-listener with link-tls")
        .to_path_buf()
}

/// Build native-rs-action-server (cached)
pub fn build_native_action_server() -> TestResult<&'static Path> {
    NATIVE_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/action-server", "action-server", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build native-rs-action-client (cached)
pub fn build_native_action_client() -> TestResult<&'static Path> {
    NATIVE_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/action-client", "action-client", Rmw::Zenoh)
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
            let example_dir = root.join("examples/native/rust/talker");
            let target_dir = example_dir.join("target-safety");
            let binary_path = target_dir.join(format!("{}/talker", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
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
            let example_dir = root.join("examples/native/rust/listener");
            let target_dir = example_dir.join("target-safety");
            let binary_path = target_dir.join(format!("{}/listener", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
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
            let example_dir = root.join("examples/native/rust/listener");
            let target_dir = example_dir.join("target-zero-copy");
            let binary_path = target_dir.join(format!("{}/listener", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
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
            build_example_rmw("native/rust/service-server", "service-server", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build native-rs-service-client (cached)
pub fn build_native_service_client() -> TestResult<&'static Path> {
    NATIVE_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/service-client", "service-client", Rmw::Zenoh)
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
        .get_or_try_init(|| build_example("native/rust/custom-msg", "custom_msg", None, None))
        .map(|p| p.as_path())
}

/// Build native-rs-custom-msg (uncached, for serialization tests)
pub fn build_native_custom_msg_no_zenoh() -> TestResult<PathBuf> {
    build_example("native/rust/custom-msg", "custom_msg", None, None)
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
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-bsp-talker"))
        .map(|p| p.as_path())
}

/// Build qemu-bsp-listener (cached)
pub fn build_qemu_bsp_listener() -> TestResult<&'static Path> {
    QEMU_BSP_LISTENER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-bsp-listener"))
        .map(|p| p.as_path())
}

// Phase 169.4 — bare-metal DDS fixture builders deleted with the
// Rust DDS examples (Phase 169.2).
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
// Serial Example Builders (QEMU bare-metal, cross-compiled)
// ═══════════════════════════════════════════════════════════════════════════

/// Build qemu-serial-talker (cached)
pub fn build_qemu_serial_talker() -> TestResult<&'static Path> {
    QEMU_SERIAL_TALKER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-serial-talker"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-serial-talker binary path
#[rstest::fixture]
pub fn qemu_serial_talker_binary() -> PathBuf {
    build_qemu_serial_talker()
        .expect("Failed to build qemu-serial-talker")
        .to_path_buf()
}

/// Build qemu-serial-listener (cached)
pub fn build_qemu_serial_listener() -> TestResult<&'static Path> {
    QEMU_SERIAL_LISTENER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-serial-listener"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-serial-listener binary path
#[rstest::fixture]
pub fn qemu_serial_listener_binary() -> PathBuf {
    build_qemu_serial_listener()
        .expect("Failed to build qemu-serial-listener")
        .to_path_buf()
}

/// Phase 207 — build the bare-metal XRCE talker (cached). Wraps the same
/// `build_example` path the serial-talker uses; the prebuilt at
/// `target/.../<profile>/qemu-talker-xrce` is checked, not rebuilt
/// (`just qemu build-fixtures` / `cargo build --profile <p>` is the build
/// step, this is the resolve step).
pub fn build_qemu_talker_xrce() -> TestResult<&'static Path> {
    QEMU_TALKER_XRCE_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-talker-xrce"))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the qemu-talker-xrce binary path.
#[rstest::fixture]
pub fn qemu_talker_xrce_binary() -> PathBuf {
    build_qemu_talker_xrce()
        .expect("Failed to build qemu-talker-xrce")
        .to_path_buf()
}

// ═══════════════════════════════════════════════════════════════════════════
// RTIC Example Builders (STM32F4, cross-compiled)
// ═══════════════════════════════════════════════════════════════════════════

/// Cached path to the stm32f4-rtic-talker binary
static RTIC_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the stm32f4-rtic-listener binary
static RTIC_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-talker binary
static NATIVE_RTIC_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-listener binary
static NATIVE_RTIC_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-service-server binary
static NATIVE_RTIC_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-service-client binary
static NATIVE_RTIC_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the stm32f4-rtic-service-server binary
static RTIC_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the stm32f4-rtic-service-client binary
static RTIC_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-action-server binary
static NATIVE_RTIC_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the native rtic-action-client binary
static NATIVE_RTIC_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the stm32f4-rtic-action-server binary
static RTIC_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the stm32f4-rtic-action-client binary
static RTIC_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Resolve the prebuilt stm32f4 RTIC talker fixture (cached). The
/// `examples/stm32f4/rust/talker-rtic` crate's `[[bin]]` is named
/// `stm32f4-rs-rtic-example` (not `stm32f4-rtic-talker` — the old name here was
/// stale and matched nothing, so this accessor never resolved).
pub fn build_rtic_talker() -> TestResult<&'static Path> {
    RTIC_TALKER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rs-rtic-example"))
        .map(|p| p.as_path())
}

/// Build stm32f4-rtic-listener (cached)
pub fn build_rtic_listener() -> TestResult<&'static Path> {
    RTIC_LISTENER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rtic-listener"))
        .map(|p| p.as_path())
}

/// Build native rtic-talker (cached)
pub fn build_native_rtic_talker() -> TestResult<&'static Path> {
    NATIVE_RTIC_TALKER_BINARY
        .get_or_try_init(|| build_example("native/rust/talker-rtic", "rtic-talker", None, None))
        .map(|p| p.as_path())
}

/// Build native rtic-listener (cached)
pub fn build_native_rtic_listener() -> TestResult<&'static Path> {
    NATIVE_RTIC_LISTENER_BINARY
        .get_or_try_init(|| build_example("native/rust/listener-rtic", "rtic-listener", None, None))
        .map(|p| p.as_path())
}

/// Build native rtic-service-server (cached)
pub fn build_native_rtic_service_server() -> TestResult<&'static Path> {
    NATIVE_RTIC_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/service-server-rtic",
                "rtic-service-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native rtic-service-client (cached)
pub fn build_native_rtic_service_client() -> TestResult<&'static Path> {
    NATIVE_RTIC_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/service-client-rtic",
                "rtic-service-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build stm32f4-rtic-service-server (cached)
pub fn build_rtic_service_server() -> TestResult<&'static Path> {
    RTIC_SERVICE_SERVER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rtic-service-server"))
        .map(|p| p.as_path())
}

/// Build stm32f4-rtic-service-client (cached)
pub fn build_rtic_service_client() -> TestResult<&'static Path> {
    RTIC_SERVICE_CLIENT_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rtic-service-client"))
        .map(|p| p.as_path())
}

/// Build native rtic-action-server (cached)
pub fn build_native_rtic_action_server() -> TestResult<&'static Path> {
    NATIVE_RTIC_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/action-server-rtic",
                "rtic-action-server",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build native rtic-action-client (cached)
pub fn build_native_rtic_action_client() -> TestResult<&'static Path> {
    NATIVE_RTIC_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example(
                "native/rust/action-client-rtic",
                "rtic-action-client",
                None,
                None,
            )
        })
        .map(|p| p.as_path())
}

/// Build stm32f4-rtic-action-server (cached)
pub fn build_rtic_action_server() -> TestResult<&'static Path> {
    RTIC_ACTION_SERVER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rtic-action-server"))
        .map(|p| p.as_path())
}

/// Build stm32f4-rtic-action-client (cached)
pub fn build_rtic_action_client() -> TestResult<&'static Path> {
    RTIC_ACTION_CLIENT_BINARY
        // Phase 226.D — built into build/fixtures-cargo/stm32f4.
        .get_or_try_init(|| require_stm32f4_fixture("stm32f4-rtic-action-client"))
        .map(|p| p.as_path())
}

// ═══════════════════════════════════════════════════════════════════════════
// XRCE-DDS Example Builders
// ═══════════════════════════════════════════════════════════════════════════

/// Build the xrce-talker example binary (cached).
pub fn build_xrce_talker() -> TestResult<&'static Path> {
    XRCE_TALKER_BINARY
        .get_or_try_init(|| build_example_rmw("native/rust/talker", "talker", Rmw::Xrce))
        .map(|p| p.as_path())
}

/// Build the xrce-listener example binary (cached).
pub fn build_xrce_listener() -> TestResult<&'static Path> {
    XRCE_LISTENER_BINARY
        .get_or_try_init(|| build_example_rmw("native/rust/listener", "listener", Rmw::Xrce))
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

/// Resolve the prebuilt px4-stub example binary (Phase 233.4). Built by
/// `just px4 build-fixtures` to `examples/px4/rust/xrce/px4-stub/target-xrce/`.
pub fn build_px4_stub() -> TestResult<&'static Path> {
    PX4_STUB_BINARY
        .get_or_try_init(|| build_example_rmw("px4/rust/xrce/px4-stub", "px4-stub", Rmw::Xrce))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the px4-stub binary path.
#[rstest::fixture]
pub fn px4_stub_binary() -> PathBuf {
    build_px4_stub()
        .expect("Failed to build px4-stub")
        .to_path_buf()
}

/// Resolve the prebuilt px4 offboard-companion example binary (Phase 233.4).
pub fn build_px4_companion() -> TestResult<&'static Path> {
    PX4_COMPANION_BINARY
        .get_or_try_init(|| {
            build_example_rmw(
                "px4/rust/xrce/offboard-companion",
                "offboard-companion",
                Rmw::Xrce,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the px4 offboard-companion binary path.
#[rstest::fixture]
pub fn px4_companion_binary() -> PathBuf {
    build_px4_companion()
        .expect("Failed to build px4 offboard-companion")
        .to_path_buf()
}

/// Build the xrce-service-server example binary (cached).
pub fn build_xrce_service_server() -> TestResult<&'static Path> {
    XRCE_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/service-server", "service-server", Rmw::Xrce)
        })
        .map(|p| p.as_path())
}

/// Build the xrce-service-client example binary (cached).
pub fn build_xrce_service_client() -> TestResult<&'static Path> {
    XRCE_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/service-client", "service-client", Rmw::Xrce)
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
            build_example_rmw("native/rust/action-server", "action-server", Rmw::Xrce)
        })
        .map(|p| p.as_path())
}

/// Build the xrce-action-client example binary (cached).
pub fn build_xrce_action_client() -> TestResult<&'static Path> {
    XRCE_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_rmw("native/rust/action-client", "action-client", Rmw::Xrce)
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
                "native/rust/serial-talker",
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
                "native/rust/serial-listener",
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
            build_test_fixture("nros-bench/large-msg-xrce", "xrce-large-msg-test", None)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-large-msg-test binary path.
///
/// Phase 150.F — "binary not prebuilt" is an environment/setup
/// condition (user didn't run `just build-test-fixtures`), not a
/// test-logic failure. Surface it via `nros_tests::skip!` so
/// `_count-real-failures` filters it out and the ci summary
/// doesn't flag it as a real failure. Any OTHER build error (e.g.
/// the fixture crate genuinely failing to compile) panics
/// normally and counts as a real failure.
#[rstest::fixture]
pub fn xrce_large_msg_test_binary() -> PathBuf {
    match build_xrce_large_msg_test() {
        Ok(p) => p.to_path_buf(),
        Err(crate::TestError::BuildFailed(msg)) if msg.contains("not prebuilt") => {
            nros_tests_skip(msg)
        }
        Err(e) => panic!("Failed to build xrce-large-msg-test: {e:?}"),
    }
}

/// Helper that panics with the `[SKIPPED]` prefix recognised by
/// `justfile::_count-real-failures`. Kept local to this module
/// so the macro's lexical scope doesn't need to escape.
fn nros_tests_skip(msg: String) -> ! {
    panic!("[SKIPPED] {msg}")
}

// ═══════════════════════════════════════════════════════════════════════════
// Stress Test & Large Message Builders
// ═══════════════════════════════════════════════════════════════════════════

/// Build the zenoh-stress-test binary (cached).
pub fn build_zenoh_stress_test() -> TestResult<&'static Path> {
    ZENOH_STRESS_TEST_BINARY
        .get_or_try_init(|| {
            build_test_fixture("nros-bench/stress-zenoh", "zenoh-stress-test", None)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the zenoh-stress-test binary path.
/// See `xrce_large_msg_test_binary` for the not-prebuilt → skip
/// rationale (Phase 150.F).
#[rstest::fixture]
pub fn zenoh_stress_test_binary() -> PathBuf {
    match build_zenoh_stress_test() {
        Ok(p) => p.to_path_buf(),
        Err(crate::TestError::BuildFailed(msg)) if msg.contains("not prebuilt") => {
            nros_tests_skip(msg)
        }
        Err(e) => panic!("Failed to build zenoh-stress-test: {e:?}"),
    }
}

/// Build the zenoh-stress-test binary with large subscriber buffer (8192B, cached).
///
/// Uses `ZPICO_SUBSCRIBER_BUFFER_SIZE=8192` and a separate `target-large-buf`
/// directory to avoid overwriting the default stress-test binary.
pub fn build_zenoh_stress_test_large_buf() -> TestResult<&'static Path> {
    ZENOH_STRESS_TEST_LARGE_BUF_BINARY
        .get_or_try_init(|| {
            let root = project_root();
            let example_dir = root.join("packages/testing/nros-bench/stress-zenoh");
            let target_dir = example_dir.join("target-large-buf");
            let binary_path =
                target_dir.join(format!("{}/zenoh-stress-test", cargo_target_profile_dir()));
            require_prebuilt_binary(&binary_path)
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
        .get_or_try_init(|| build_test_fixture("nros-bench/stress-xrce", "xrce-stress-test", None))
        .map(|p| p.as_path())
}

/// rstest fixture that provides the xrce-stress-test binary path.
/// See `xrce_large_msg_test_binary` for the not-prebuilt → skip
/// rationale (Phase 150.F).
#[rstest::fixture]
pub fn xrce_stress_test_binary() -> PathBuf {
    match build_xrce_stress_test() {
        Ok(p) => p.to_path_buf(),
        Err(crate::TestError::BuildFailed(msg)) if msg.contains("not prebuilt") => {
            nros_tests_skip(msg)
        }
        Err(e) => panic!("Failed to build xrce-stress-test: {e:?}"),
    }
}

/// Build qemu-bsp-large-msg-test (cached).
pub fn build_qemu_large_msg_test() -> TestResult<&'static Path> {
    QEMU_LARGE_MSG_TEST_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-bsp-large-msg-test"))
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

/// Build c-talker example (cached)
pub fn build_c_talker() -> TestResult<&'static Path> {
    C_TALKER_BINARY
        .get_or_try_init(|| build_example_cmake_rmw("native/c/talker", "c_talker", Rmw::Zenoh))
        .map(|p| p.as_path())
}

/// Build c-listener example (cached)
pub fn build_c_listener() -> TestResult<&'static Path> {
    C_LISTENER_BINARY
        .get_or_try_init(|| build_example_cmake_rmw("native/c/listener", "c_listener", Rmw::Zenoh))
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
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/c/service-server", "c_service_server", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build c-service-client example (cached)
pub fn build_c_service_client() -> TestResult<&'static Path> {
    C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/c/service-client", "c_service_client", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build c-action-server example (cached)
pub fn build_c_action_server() -> TestResult<&'static Path> {
    C_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/c/action-server", "c_action_server", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build c-action-client example (cached)
pub fn build_c_action_client() -> TestResult<&'static Path> {
    C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/c/action-client", "c_action_client", Rmw::Zenoh)
        })
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

/// Build c-xrce-talker example (cached)
pub fn build_c_xrce_talker() -> TestResult<&'static Path> {
    C_XRCE_TALKER_BINARY
        .get_or_try_init(|| build_example_cmake_rmw("native/c/talker", "c_talker", Rmw::Xrce))
        .map(|p| p.as_path())
}

/// Build c-xrce-listener example (cached)
pub fn build_c_xrce_listener() -> TestResult<&'static Path> {
    C_XRCE_LISTENER_BINARY
        .get_or_try_init(|| build_example_cmake_rmw("native/c/listener", "c_listener", Rmw::Xrce))
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

/// Build an ESP32-C3 QEMU example using the pinned nightly
///
/// ESP32 examples require a nightly toolchain with `-Zbuild-std`, so we
/// can't use the generic `build_example()` which uses stable `cargo build`.
/// The channel comes from `tools/rust-toolchain.toml` via [`pinned_nightly`].
fn build_esp32_qemu_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-esp32-baremetal/rust/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "ESP32 example directory not found: {}",
            example_dir.display()
        )));
    }

    eprintln!("Building qemu-esp32/rust/{}...", name);

    let mut args = vec![format!("+{}", pinned_nightly())];
    args.extend(cargo_build_args());
    let output = cmd("cargo", args)
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
        "target/riscv32imc-unknown-none-elf/{}/{}",
        cargo_target_profile_dir(),
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

/// Resolve the PREBUILT ESP32-C3 QEMU workspace Entry ELF (Phase 225.O).
///
/// The workspace Entry (`examples/workspaces/rust/src/esp32_entry`) is
/// the ESP32 sibling of the native / FreeRTOS / ThreadX / Zephyr
/// workspace Entries: a SINGLE bare-metal binary that hosts the whole
/// launch-defined node set (talker + listener) in one image via
/// `nros::main!(launch = "demo_bringup:system.launch.xml")`. It is built
/// by the workspace-fixture lane
/// (`scripts/build/workspace-fixtures-build.sh esp32 rust`, run by
/// `just esp32 build-examples` / `build-fixtures`) into
/// `target-fixtures/esp32/riscv32imc-unknown-none-elf/<profile>/esp32_entry`,
/// NOT in-body — tests only run prebuilt workspace fixtures, mirroring
/// the Zephyr workspace Entry convention.
///
/// Fails fast with a `just esp32 build-fixtures` hint when the binary is
/// absent.
pub fn get_prebuilt_esp32_qemu_workspace_entry() -> TestResult<PathBuf> {
    let root = project_root();
    let elf = root.join(format!(
        "examples/workspaces/rust/target-fixtures/esp32/riscv32imc-unknown-none-elf/{}/esp32_entry",
        cargo_target_profile_dir()
    ));
    if !elf.exists() {
        return Err(TestError::BuildFailed(format!(
            "ESP32 workspace Entry binary not found: {}\n\
             Build the workspace fixtures first: `just esp32 build-fixtures` \
             (or `bash scripts/build/workspace-fixtures-build.sh esp32 rust`).",
            elf.display()
        )));
    }
    Ok(elf)
}

// ───────────────────────────────────────────────────────────────────────────
// Phase 169.4b — ESP32-C3 QEMU Rust DDS fixture builders deleted
// alongside the Rust DDS retirement (Phase 169.2 deleted the example
// crates).
// ───────────────────────────────────────────────────────────────────────────

// ═══════════════════════════════════════════════════════════════════════════
// RTIC QEMU Example Builders (MPS2-AN385, Cortex-M3)
// ═══════════════════════════════════════════════════════════════════════════

/// Cached path to the qemu-rtic-talker binary
static QEMU_RTIC_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-rtic-listener binary
static QEMU_RTIC_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build qemu-rtic-talker (cached)
pub fn build_qemu_rtic_talker() -> TestResult<&'static Path> {
    QEMU_RTIC_TALKER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-talker"))
        .map(|p| p.as_path())
}

/// Build qemu-rtic-listener (cached)
pub fn build_qemu_rtic_listener() -> TestResult<&'static Path> {
    QEMU_RTIC_LISTENER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-listener"))
        .map(|p| p.as_path())
}

/// Cached path to the qemu-rtic-service-server binary
static QEMU_RTIC_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-rtic-service-client binary
static QEMU_RTIC_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build qemu-rtic-service-server (cached)
pub fn build_qemu_rtic_service_server() -> TestResult<&'static Path> {
    QEMU_RTIC_SERVICE_SERVER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-service-server"))
        .map(|p| p.as_path())
}

/// Build qemu-rtic-service-client (cached)
pub fn build_qemu_rtic_service_client() -> TestResult<&'static Path> {
    QEMU_RTIC_SERVICE_CLIENT_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-service-client"))
        .map(|p| p.as_path())
}

// ═══════════════════════════════════════════════════════════════════════════
// C++ Example Builders (CMake-based)
// ═══════════════════════════════════════════════════════════════════════════

/// Cached path to the cpp-talker binary
static CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-listener binary
static CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-service-server binary
static CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-service-client binary
static CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-action-server binary
static CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-action-client binary
static CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the cpp-parameters binary
static CPP_PARAMETERS_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build cpp-talker example (cached)
pub fn build_cpp_talker() -> TestResult<&'static Path> {
    CPP_TALKER_BINARY
        .get_or_try_init(|| build_example_cmake_rmw("native/cpp/talker", "cpp_talker", Rmw::Zenoh))
        .map(|p| p.as_path())
}

/// Build cpp-listener example (cached)
pub fn build_cpp_listener() -> TestResult<&'static Path> {
    CPP_LISTENER_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/cpp/listener", "cpp_listener", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build cpp-service-server example (cached)
pub fn build_cpp_service_server() -> TestResult<&'static Path> {
    CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw(
                "native/cpp/service-server",
                "cpp_service_server",
                Rmw::Zenoh,
            )
        })
        .map(|p| p.as_path())
}

/// Build cpp-service-client example (cached)
pub fn build_cpp_service_client() -> TestResult<&'static Path> {
    CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw(
                "native/cpp/service-client",
                "cpp_service_client",
                Rmw::Zenoh,
            )
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the cpp-talker binary path
#[rstest::fixture]
pub fn cpp_talker_binary() -> PathBuf {
    build_cpp_talker()
        .expect("Failed to build cpp-talker")
        .to_path_buf()
}

/// rstest fixture that provides the cpp-listener binary path
#[rstest::fixture]
pub fn cpp_listener_binary() -> PathBuf {
    build_cpp_listener()
        .expect("Failed to build cpp-listener")
        .to_path_buf()
}

/// rstest fixture that provides the cpp-service-server binary path
#[rstest::fixture]
pub fn cpp_service_server_binary() -> PathBuf {
    build_cpp_service_server()
        .expect("Failed to build cpp-service-server")
        .to_path_buf()
}

/// rstest fixture that provides the cpp-service-client binary path
#[rstest::fixture]
pub fn cpp_service_client_binary() -> PathBuf {
    build_cpp_service_client()
        .expect("Failed to build cpp-service-client")
        .to_path_buf()
}

/// Build cpp-action-server example (cached)
pub fn build_cpp_action_server() -> TestResult<&'static Path> {
    CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/cpp/action-server", "cpp_action_server", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// Build cpp-action-client example (cached)
pub fn build_cpp_action_client() -> TestResult<&'static Path> {
    CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/cpp/action-client", "cpp_action_client", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the cpp-action-server binary path
#[rstest::fixture]
pub fn cpp_action_server_binary() -> PathBuf {
    build_cpp_action_server()
        .expect("Failed to build cpp-action-server")
        .to_path_buf()
}

/// rstest fixture that provides the cpp-action-client binary path
#[rstest::fixture]
pub fn cpp_action_client_binary() -> PathBuf {
    build_cpp_action_client()
        .expect("Failed to build cpp-action-client")
        .to_path_buf()
}

/// Build cpp-parameters example (cached)
pub fn build_cpp_parameters() -> TestResult<&'static Path> {
    CPP_PARAMETERS_BINARY
        .get_or_try_init(|| {
            build_example_cmake_rmw("native/cpp/parameters", "cpp_parameters", Rmw::Zenoh)
        })
        .map(|p| p.as_path())
}

/// rstest fixture that provides the cpp-parameters binary path
#[rstest::fixture]
pub fn cpp_parameters_binary() -> PathBuf {
    build_cpp_parameters()
        .expect("Failed to build cpp-parameters")
        .to_path_buf()
}

/// Cached path to the qemu-rtic-action-server binary
static QEMU_RTIC_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-rtic-action-client binary
static QEMU_RTIC_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build qemu-rtic-action-server (cached)
pub fn build_qemu_rtic_action_server() -> TestResult<&'static Path> {
    QEMU_RTIC_ACTION_SERVER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-action-server"))
        .map(|p| p.as_path())
}

/// Build qemu-rtic-action-client (cached)
pub fn build_qemu_rtic_action_client() -> TestResult<&'static Path> {
    QEMU_RTIC_ACTION_CLIENT_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-action-client"))
        .map(|p| p.as_path())
}

// ═══════════════════════════════════════════════════════════════════════════
// QEMU RTIC Mixed-Priority Example Builders (ffi-sync)
// ═══════════════════════════════════════════════════════════════════════════

/// Cached path to the qemu-rtic-mixed-talker binary
static QEMU_RTIC_MIXED_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to the qemu-rtic-mixed-listener binary
static QEMU_RTIC_MIXED_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Build qemu-rtic-mixed-talker (cached)
pub fn build_qemu_rtic_mixed_talker() -> TestResult<&'static Path> {
    QEMU_RTIC_MIXED_TALKER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-mixed-talker"))
        .map(|p| p.as_path())
}

/// Build qemu-rtic-mixed-listener (cached)
pub fn build_qemu_rtic_mixed_listener() -> TestResult<&'static Path> {
    QEMU_RTIC_MIXED_LISTENER_BINARY
        // Phase 226.D — built into build/fixtures-cargo/qemu-arm-baremetal.
        .get_or_try_init(|| require_qemu_baremetal_fixture("qemu-rtic-mixed-listener"))
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
