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

use std::{fs, process::Command, time::Duration};

#[test]
fn cmake_workspace_metadata_emits_components_cmake() -> nros_tests::TestResult<()> {
    // The cmake configure runs in the build stage — the `metadata_cpp` cmake
    // fixture (compile-check-fixtures.sh) configures examples/workspaces/cpp and
    // the §212.L cmake fns emit nros-metadata.json. This test inspects the
    // prebuilt JSON instead of running cmake at run time (issue 0034 / 0041).
    let metadata =
        nros_tests::fixtures::require_cmake_fixture("metadata_cpp", "nros-metadata.json")?;
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
    Ok(())
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
fn rust_workspace_entry_runs_prebuilt_pubsub_e2e() {
    if !nros_tests::fixtures::require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = nros_tests::fixtures::build_native_workspace_rust_entry()
        .expect("native Rust workspace Entry fixture");
    let talker = nros_tests::fixtures::build_native_talker()
        .expect("native Rust talker fixture for workspace E2E publisher");
    let router = nros_tests::fixtures::ZenohRouter::start_unique().expect("start zenohd");

    let mut cmd = Command::new(entry);
    cmd.env("NROS_LOCATOR", router.locator())
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", "8000")
        .env("NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS", "1");
    let mut proc =
        nros_tests::process::ManagedProcess::spawn_command(cmd, "rust workspace native_entry")
            .expect("spawn Rust workspace Entry fixture");

    std::thread::sleep(Duration::from_millis(700));
    let mut talker_cmd = Command::new(talker);
    talker_cmd
        .env("RUST_LOG", "info")
        .env("NROS_LOCATOR", router.locator())
        .env("NROS_SESSION_MODE", "client");
    let mut talker_proc =
        nros_tests::process::ManagedProcess::spawn_command(talker_cmd, "native talker publisher")
            .expect("spawn native talker publisher");

    let mut output = proc
        .wait_for_output_pattern("nros: hosted spin complete", Duration::from_secs(12))
        .expect("Rust workspace Entry did not report hosted spin completion");
    talker_proc.kill();
    output.push_str(
        &proc
            .wait_for_all_output(Duration::from_secs(2))
            .unwrap_or_default(),
    );

    let message_callbacks = parse_counter(&output, "message_callbacks=")
        .expect("hosted spin output should include message_callbacks counter");
    assert!(
        message_callbacks >= 1,
        "Rust workspace should observe at least one std_msgs/Int32 subscription callback; output:\n{output}"
    );
    assert!(
        output.contains("nros: application complete"),
        "Rust workspace should exit cleanly after bounded spin; output:\n{output}"
    );
}

#[test]
fn cmake_cpp_workspace_entry_starts_prebuilt_runtime() {
    let entry = nros_tests::fixtures::build_native_workspace_cpp_entry()
        .expect("native C++ workspace Entry fixture");
    let mut proc =
        nros_tests::process::ManagedProcess::spawn(entry, &[], "C++ workspace native_entry")
            .expect("spawn C++ workspace Entry fixture");

    std::thread::sleep(Duration::from_millis(300));
    assert!(
        proc.is_running(),
        "C++ workspace Entry should enter its native spin loop when started from the prebuilt binary"
    );
    proc.kill();
}

fn parse_counter(output: &str, key: &str) -> Option<usize> {
    let start = output.rfind(key)? + key.len();
    let value = output[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse().ok()
}
