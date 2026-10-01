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

use nros_tests::fixtures::RequireFixture;
use std::{fs, process::Command, time::Duration};

#[test]
fn cmake_workspace_metadata_emits_components_cmake() -> nros_tests::TestResult<()> {
    // The cmake configure runs in the build stage — the `metadata_cpp` cmake
    // fixture (compile-check-fixtures.sh) configures examples/workspaces/cpp and
    // the §212.L cmake fns emit nros-metadata.json. This test inspects the
    // prebuilt JSON instead of running cmake at run time (issue 0034 / 0041).
    let metadata =
        // `nros build` writes the cmake root under `build/<coord>/cmake`, and
        // `nros-metadata.json` lands in that binary dir (phase-445 W5 removed
        // the template's own root, which used to put it at the fixture root).
        nros_tests::fixtures::require_cmake_fixture(
            "metadata_cpp",
            "build/posix-zenoh-native/cmake/nros-metadata.json",
        )?;
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
    // `native_entry` — the entry `nros build` GENERATES for the template's
    // `[image.native]`. phase-383 W10.a moved this fixture onto the template
    // because it kept a hand-written root; phase-445 W5 removed that root too
    // (RFC-0098 D9), and with it the hand-written `robot_entry`. The property is
    // unchanged: the metadata lists the APPLICATION entry alongside the node
    // components, and the two component assertions above hold verbatim because
    // the template uses the same talker/listener.
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
        .require("native Rust workspace Entry");
    assert!(
        entry.is_file(),
        "missing Rust workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_pure_cpp_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_cpp_entry()
        .require("native C++ workspace Entry");
    assert!(
        entry.is_file(),
        "missing C++ workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_mixed_c_cpp_workspace_entry_fixture_is_prebuilt() {
    let entry = nros_tests::fixtures::build_native_workspace_mixed_entry()
        .require("native mixed C/C++ workspace Entry");
    assert!(
        entry.is_file(),
        "missing mixed C/C++ workspace Entry pkg binary at {}",
        entry.display()
    );
}

#[test]
fn cmake_pure_c_workspace_entry_fixture_is_prebuilt() {
    let entry =
        nros_tests::fixtures::build_native_workspace_c_entry().require("native C workspace Entry");
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
        .require("native Rust workspace Entry");
    let talker = nros_tests::fixtures::build_native_talker()
        .require("native Rust talker for workspace E2E publisher");
    let router = nros_tests::fixtures::or_skip(nros_tests::fixtures::ZenohRouter::start_unique());

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
        .require("native C++ workspace Entry");
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

/// Issue 1419 -- the typed single-executor C++ native entry IS a census
/// producer: `$NROS_CENSUS_OUT` makes it write what the recorder saw and exit,
/// with no router and no spin.
///
/// The generated C++ entry calls the header-only
/// `nros::board::LinuxBoard::run_components`, which never reached the Rust
/// funnel phase-463 W2 put the census switch in. Before 1419 this binary
/// ignored the variable, dialled zenoh and exited 156 on `ConnectionFailed`
/// with no file written -- so no in-tree C++ workspace could produce the
/// census a cross configure checks. This is that road, on the prebuilt
/// fixture.
///
/// The EXIT CODE is asserted only in one direction. Issue 1600 sizes this
/// configure's shared runtime from the LAST entry's model (`max_cbs` 1 where
/// `native_entry` needs 2), so the listener's subscription currently stops
/// setup at `ExecutorFull` and the run exits non-zero AFTER writing what it
/// recorded -- an incomplete census, which is the documented shape. What must
/// hold either way: the file exists, the talker's publisher and timer are in
/// it, and a run that exits 0 recorded the listener's subscription too.
#[test]
fn cmake_cpp_workspace_entry_writes_a_census_without_a_router() {
    let entry = nros_tests::fixtures::build_native_workspace_cpp_entry()
        .require("native C++ workspace Entry");
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("census.json");

    let mut child = Command::new(entry)
        .env("NROS_CENSUS_OUT", &out)
        .env_remove("NROS_ENTRY_SPIN_MS")
        .spawn()
        .expect("spawn C++ workspace Entry fixture");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait on the entry") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "census run of {} did not exit within 20 s -- it booted normally instead of \
                 writing a census (the header runner ignored $NROS_CENSUS_OUT)",
                entry.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let raw = fs::read_to_string(&out).unwrap_or_else(|e| {
        panic!(
            "census run exited {status} and wrote no census at {}: {e}",
            out.display()
        )
    });
    let census: serde_json::Value = serde_json::from_str(&raw).expect("census is JSON");
    let nodes = census["nodes"].as_array().expect("census has nodes");
    let node = |name: &str| {
        nodes
            .iter()
            .find(|n| n["id"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("census has no node `{name}`: {raw}"))
    };
    let topics = |n: &serde_json::Value, field: &str| -> Vec<String> {
        n[field]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r["unresolved_topic"]["value"].as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let talker = node("talker");
    assert!(
        topics(talker, "publishers")
            .iter()
            .any(|t| t.ends_with("chatter")),
        "the talker's publisher was not recorded: {raw}"
    );
    assert_eq!(
        talker["timers"].as_array().map(Vec::len),
        Some(1),
        "the talker's one timer was not recorded: {raw}"
    );
    if status.success() {
        assert!(
            topics(node("listener"), "subscribers")
                .iter()
                .any(|t| t.ends_with("chatter")),
            "a census run that exited 0 must have recorded the listener's subscription: {raw}"
        );
    }
}

fn parse_counter(output: &str, key: &str) -> Option<usize> {
    let start = output.rfind(key)? + key.len();
    let value = output[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse().ok()
}
