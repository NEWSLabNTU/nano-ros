//! Phase 212.D / 212.M.10 — cmake-fn metadata tests.
//!
//! Originally covered the pre-212 `nano_ros_workspace_metadata()` fn
//! (sidecar-TOML / `nros-plan.json` shape). §212.L retired that path
//! in favour of `nano_ros_node_register(...)` / `nano_ros_entry(...)`
//! / `nano_ros_deploy(...)`, which emit
//! `${CMAKE_BINARY_DIR}/nros-metadata.json` directly. Phase 212.M.10
//! migrated the native CMake workspace coverage to the promoted
//! `examples/workspaces/*` examples and these assertions follow suit.
//!
//! Coverage points:
//! 1. `cmake_workspace_metadata_emits_components_cmake` — configure
//!    produces `${CMAKE_BINARY_DIR}/nros-metadata.json` containing the
//!    expected component + application + deploy entries.
//! 2. Workspace Entry pkg fixture tests — require binaries produced by
//!    `just native build-workspace-fixtures`; tests do not run Cargo or
//!    CMake build steps.
//!
//! The metadata diagnostic skips cleanly via `nros_tests::skip!` if the
//! `nros` CLI or `cmake` aren't available — mirrors
//! `cmake_add_subdirectory_smoke`'s pattern. The fixture checks fail loud
//! with the standard prebuilt-fixture hint when the build-fixtures stage
//! has not run.

use std::{fs, path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn workspace_example(name: &str) -> PathBuf {
    workspace_root().join("examples/workspaces").join(name)
}

fn require_test_prereqs() -> Option<()> {
    if !nros_tests::require_nros_cli() {
        return None;
    }
    if !nros_tests::process::require_cmake() {
        return None;
    }
    Some(())
}

#[test]
fn cmake_workspace_metadata_emits_components_cmake() {
    if require_test_prereqs().is_none() {
        nros_tests::skip!("prereqs missing (nros CLI / cmake)");
    }

    let root = workspace_example("cpp");
    let build = tempfile::tempdir().expect("build tempdir");
    let build_dir = build.path().join("cpp");

    let out = Command::new("cmake")
        .arg("-S")
        .arg(&root)
        .arg("-B")
        .arg(&build_dir)
        .output()
        .expect("spawn cmake configure");
    assert!(
        out.status.success(),
        "cmake configure failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // §212.L.9 cmake fns emit a single metadata JSON at the build root.
    let metadata = build_dir.join("nros-metadata.json");
    assert!(
        metadata.is_file(),
        "expected {} to be emitted by the §212.L cmake fns",
        metadata.display()
    );
    let body = fs::read_to_string(&metadata).expect("read nros-metadata.json");
    assert!(
        body.contains("\"name\": \"talker\"") && body.contains("\"class\": \"talker_pkg::Talker\""),
        "metadata missing talker component entry:\n{body}"
    );
    assert!(
        body.contains("\"name\": \"listener\"")
            && body.contains("\"class\": \"listener_pkg::Listener\""),
        "metadata missing listener component entry:\n{body}"
    );
    assert!(
        body.contains("\"name\": \"native_entry\""),
        "metadata missing native_entry application entry:\n{body}"
    );
    assert!(
        body.contains("\"native\""),
        "metadata missing native deploy target:\n{body}"
    );
}

#[test]
fn rust_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_rust_entry()
        .expect("native Rust workspace Entry fixture");
    assert!(
        entry.is_file(),
        "missing Rust workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_pure_cpp_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_cpp_entry()
        .expect("native C++ workspace Entry fixture");
    assert!(
        entry.is_file(),
        "missing C++ workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_mixed_c_cpp_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_mixed_entry()
        .expect("native mixed C/C++ workspace Entry fixture");
    assert!(
        entry.is_file(),
        "missing mixed C/C++ workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_pure_c_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_c_entry()
        .expect("native C workspace Entry fixture");
    assert!(
        entry.is_file(),
        "missing C workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
#[ignore = "workspace entries do not yet expose bounded success output for E2E assertions"]
fn native_workspace_runtime_e2e_observability_gap() {
    let entry = nros_tests::fixtures::build_native_workspace_rust_entry()
        .expect("native Rust workspace Entry fixture");
    panic!(
        "TODO: run {} once workspace entries expose a deterministic \
         publish/receive success signal and bounded exit mode",
        entry.display()
    );
}
