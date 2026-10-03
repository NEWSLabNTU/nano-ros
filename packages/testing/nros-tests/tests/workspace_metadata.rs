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

/// Issue 1419 -- run a prebuilt native workspace entry in census mode and
/// assert it wrote a COMPLETE census of the talker/listener pair, with every
/// entity attributed to the node that created it.
///
/// Complete is asserted outright, exit code and all. The census executor is
/// opened at the executor's own ceilings rather than at the contract-derived
/// `MAX_CBS` (`CENSUS_SIZING` in `nros-cpp`), and issue 1600 gave the native
/// image's runtime the union of every entry's fragment, so a census run of
/// this pair has no sizing reason left to stop short. This test used to assert
/// the listener's subscription only when the run exited 0, because 1600 made
/// it exit 250 at `ExecutorFull`.
fn assert_entry_writes_a_complete_census(entry: &std::path::Path, lang: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("census.json");

    let mut child = Command::new(entry)
        .env("NROS_CENSUS_OUT", &out)
        .env_remove("NROS_ENTRY_SPIN_MS")
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {lang} workspace Entry fixture: {e}"));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait on the entry") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!(
                "census run of {} did not exit within 20 s -- it booted normally instead of \
                 writing a census (the runner ignored $NROS_CENSUS_OUT)",
                entry.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    let raw = fs::read_to_string(&out).unwrap_or_else(|e| {
        panic!(
            "{lang} census run exited {status} and wrote no census at {}: {e}",
            out.display()
        )
    });
    assert!(
        status.success(),
        "a {lang} census run of the talker/listener pair must exit 0 -- nothing about its \
         sizing can stop it short any more -- got {status}: {raw}"
    );
    let census: serde_json::Value = serde_json::from_str(&raw).expect("census is JSON");
    let nodes = census["nodes"].as_array().expect("census has nodes");
    let node = |name: &str| {
        nodes
            .iter()
            .find(|n| n["id"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("{lang} census has no node `{name}`: {raw}"))
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
    let listener = node("listener");
    assert!(
        topics(listener, "subscribers")
            .iter()
            .any(|t| t.ends_with("chatter")),
        "the listener's subscription was not recorded: {raw}"
    );
    // Attribution: a C entry's entities used to reach the recorder with no
    // node (phase-463 Limits). Each endpoint must sit under the node that
    // created it, not under the other one.
    assert!(
        topics(listener, "publishers").is_empty() && topics(talker, "subscribers").is_empty(),
        "an entity was attributed to the wrong node: {raw}"
    );
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
#[test]
fn cmake_cpp_workspace_entry_writes_a_census_without_a_router() {
    let entry = nros_tests::fixtures::build_native_workspace_cpp_entry()
        .require("native C++ workspace Entry");
    assert_entry_writes_a_complete_census(entry, "C++");
}

/// Issue 1419 -- the generated native C entry is a census producer too, with
/// node attribution.
///
/// phase-463 listed C entries as having none ("a C node's entities ... are
/// visible to the recording backend but not attributed to a node"). Measured
/// on `examples/workspaces/c` (2026-10-02), that is not true of a GENERATED C
/// entry: it runs through `nros_board_native_run_components_named_in` (issue
/// 1597), the funnel that holds the switch, it creates each node with
/// `nros_cpp_node_create` (which opens the recorder's node cursor) before
/// configuring that node's component, and a C component creates its entities
/// through the same `nros_cpp_*` C ABI the hooks sit on. So the census reads
/// `talker` (one publisher, one wall timer) and `listener` (one
/// subscription). This pins it. What stays unattributed is a C node that
/// opens its own node through `nros-c` rather than taking the entry's.
#[test]
fn cmake_c_workspace_entry_writes_a_census_without_a_router() {
    let entry =
        nros_tests::fixtures::build_native_workspace_c_entry().require("native C workspace Entry");
    assert_entry_writes_a_complete_census(entry, "C");
}

/// Issue 1419 -- the generated native RUST entry is a census producer.
///
/// Before, `nros-board-linux`'s `boot_hosted` answered `$NROS_CENSUS_OUT` with
/// a refusal ("a RUST entry has no recorder to dump"), because the census hooks
/// sat on the C++ ABI and the Rust install path crossed none of them. The hooks
/// are in `nros` now, and the generated host entry turns on
/// `nros-board-linux/census`, which selects the recording backend, opens the
/// executor at the census ceilings and writes what the recorder saw.
#[test]
fn cargo_rust_workspace_entry_writes_a_census_without_a_router() {
    let entry = nros_tests::fixtures::build_native_workspace_rust_entry()
        .require("native Rust workspace Entry");
    assert_entry_writes_a_complete_census(entry, "Rust");
}

/// Issue 1556 item 2 -- an rclc-style C APPLICATION that owns its own `main`
/// and its own spin loop is a census producer: `nros_support_init` selects the
/// recording backend, the application creates its node, publisher and timer
/// through `nros-c` (each crossing `nros::census_hooks` or the recorder), and
/// its first `rclc_executor_spin_period` writes the census and exits 0 -- no
/// router, no spin.
///
/// Before: the C API had no switch and its node / timer entry points no hooks,
/// so this binary ignored `$NROS_CENSUS_OUT`, dialled zenoh and exited on
/// `ConnectionFailed` with no file.
#[test]
fn an_rclc_c_application_writes_a_census_without_a_router() {
    let bin = nros_tests::fixtures::build_native_c_talker_rmw(nros_tests::fixtures::Rmw::Zenoh)
        .require("native C talker (zenoh)");
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("census.json");
    let status = Command::new(bin)
        .env("NROS_CENSUS_OUT", &out)
        .env_remove("NROS_ENTRY_SPIN_MS")
        .status()
        .expect("run the C talker");
    let raw = fs::read_to_string(&out)
        .unwrap_or_else(|e| panic!("exited {status} and wrote no census: {e}"));
    assert!(status.success(), "a census run exits 0: {status}");
    let census: serde_json::Value = serde_json::from_str(&raw).expect("census is JSON");
    let talker = census["nodes"]
        .as_array()
        .and_then(|n| n.iter().find(|n| n["id"].as_str() == Some("talker")))
        .unwrap_or_else(|| panic!("no `talker` node: {raw}"));
    assert!(
        talker["publishers"].as_array().is_some_and(|p| p
            .iter()
            .any(|r| r["unresolved_topic"]["value"] == "/chatter")),
        "the publisher, on its node: {raw}"
    );
    assert_eq!(
        talker["timers"].as_array().map(Vec::len),
        Some(1),
        "the wall timer, on its node: {raw}"
    );
}

/// Issue 1556 (c) -- a C++ APPLICATION that owns its own `main` (`nros::init`,
/// then its own loop) is a census producer: `nros_cpp_init_rmw` selects the
/// recording backend, the application creates its node and entities through
/// the hooked `nros_cpp_*` ABI, and its FIRST BLOCKING CALL writes the census
/// and exits 0 -- no router. Two programs, because "the first blocking call"
/// is not one function: the talker's first block is `nros::spin_once`, the
/// service client's is `wait_for_service`, which never spins.
///
/// Before: only the two board RUNNERS answered `$NROS_CENSUS_OUT`, so both
/// binaries ignored it, dialled zenoh and exited on `ConnectionFailed` with no
/// file.
#[test]
fn a_cpp_application_writes_a_census_at_its_first_blocking_call() {
    for (case, bin, node, list, topic) in [
        ("talker", "cpp_talker", "talker", "publishers", "/chatter"),
        (
            "service-client",
            "cpp_service_client",
            "add_two_ints_client",
            "service_clients",
            "/add_two_ints",
        ),
    ] {
        let exe = nros_tests::fixtures::build_native_cpp_example_rmw(
            case,
            bin,
            nros_tests::fixtures::Rmw::Zenoh,
        )
        .require("native C++ example (zenoh)");
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("census.json");
        let status = Command::new(exe)
            .env("NROS_CENSUS_OUT", &out)
            .env_remove("NROS_ENTRY_SPIN_MS")
            .status()
            .expect("run the C++ example");
        let raw = fs::read_to_string(&out)
            .unwrap_or_else(|e| panic!("{case}: exited {status} and wrote no census: {e}"));
        assert!(status.success(), "{case}: a census run exits 0: {status}");
        let census: serde_json::Value = serde_json::from_str(&raw).expect("census is JSON");
        let n = census["nodes"]
            .as_array()
            .and_then(|n| n.iter().find(|n| n["id"].as_str() == Some(node)))
            .unwrap_or_else(|| panic!("{case}: no `{node}` node: {raw}"));
        assert!(
            n[list].as_array().is_some_and(|rows| rows.iter().any(|r| {
                r["unresolved_topic"]["value"] == topic || r["unresolved_name"]["value"] == topic
            })),
            "{case}: `{topic}` under `{list}`, on its node: {raw}"
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
