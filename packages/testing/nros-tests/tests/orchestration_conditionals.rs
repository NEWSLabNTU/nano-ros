//! Phase 211.D — conditionals + `<group>` scoping deploy-time eval.
//!
//! Drives `nros plan` against the committed `fixtures/orchestration_conditionals/`
//! workspace and asserts the planner honors both:
//!
//! 1. `<node if="$(var enable_logger)">…</node>` — entity omitted when the
//!    arg resolves falsy, present when truthy.
//! 2. `<group><push_ros_namespace namespace="scoped"/>…</group>` — nested
//!    namespace prefix propagates onto the child entity's resolved
//!    `namespace` + `launch_name`.
//!
//! ## Status (audited 2026-05-31)
//!
//! Both behaviors are **already implemented** in the upstream `nros-cli`
//! planner. The phase-doc bullet "Planner change: emit only entities whose
//! `if_condition` resolves truthy" is satisfied by today's planner; what
//! 211.D was missing was an **in-tree regression gate** asserting it. This
//! test fills that gap.
//!
//! ## Pre-baked records
//!
//! Two record.json files are committed (`record-false.json` /
//! `record-true.json`), each the output of `play_launch_parser` with the
//! corresponding `enable_logger:=<bool>` launch arg. This decouples the test
//! from `play_launch_parser` being on PATH (same pattern as 211.A/B) AND
//! exercises the post-parse → planner path with both already-evaluated arg
//! values.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/orchestration_conditionals")
}

fn plan_with_record(record: &str) -> serde_json::Value {
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let fixture = fixture_dir();
    let record_path = fixture.join(record);
    assert!(
        record_path.is_file(),
        "fixture missing committed {record}: {}",
        record_path.display()
    );

    let out = tempfile::tempdir().expect("tempdir");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_cond")
        .arg("demo_cond_bringup/launch/system.launch.xml")
        .arg("--workspace")
        .arg(&fixture)
        .arg("--nros-toml")
        .arg(fixture.join("demo_cond_bringup/system.toml"))
        .arg("--record")
        .arg(&record_path)
        .arg("--metadata")
        .arg(fixture.join("_metadata/always.json"))
        .arg("--metadata")
        .arg(fixture.join("_metadata/logger.json"))
        .arg("--out-dir")
        .arg(out.path())
        .output()
        .expect("spawn nros plan");
    assert!(
        result.status.success(),
        "nros plan ({record}) exit={} stderr={}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );

    serde_json::from_str(
        &std::fs::read_to_string(out.path().join("nros-plan.json")).expect("read plan"),
    )
    .expect("parse plan")
}

#[test]
fn conditionals_disabled_omits_logger() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let plan = plan_with_record("record-false.json");
    let instances = plan["instances"].as_array().expect("instances array");

    // Only the unconditional `always` node lands when enable_logger=false —
    // the `<node if="$(var enable_logger)">` is filtered out at plan time.
    assert_eq!(
        instances.len(),
        1,
        "expected 1 instance (logger omitted), got {}: {instances:#?}",
        instances.len()
    );
    let only = &instances[0];
    assert_eq!(only["component"], "demo_cond::always", "wrong component");
    assert_eq!(only["namespace"], "/", "always node namespace");
    assert_eq!(only["launch_name"], "/always_on", "always launch_name");

    // Defense-in-depth: there must be NO instance referencing the logger
    // component, regardless of count assertion above.
    assert!(
        !instances
            .iter()
            .any(|i| i["component"].as_str() == Some("demo_cond::logger")),
        "logger instance present despite if-false: {instances:#?}"
    );
}

#[test]
fn conditionals_enabled_keeps_logger_and_scopes_namespace() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let plan = plan_with_record("record-true.json");
    let instances = plan["instances"].as_array().expect("instances array");

    assert_eq!(
        instances.len(),
        2,
        "expected 2 instances (always + logger), got {}: {instances:#?}",
        instances.len()
    );

    let always = instances
        .iter()
        .find(|i| i["component"] == "demo_cond::always")
        .expect("always instance");
    assert_eq!(always["namespace"], "/", "always namespace");
    assert_eq!(always["launch_name"], "/always_on", "always launch_name");

    let logger = instances
        .iter()
        .find(|i| i["component"] == "demo_cond::logger")
        .expect("logger instance");
    // The `<group><push_ros_namespace namespace="scoped"/>…</group>` wrap
    // must propagate onto the conditioned logger node: namespace = "/scoped"
    // and the launch_name picks up the same prefix.
    assert_eq!(
        logger["namespace"], "/scoped",
        "logger should inherit `push_ros_namespace` scope: {logger:#?}"
    );
    assert_eq!(
        logger["launch_name"], "/scoped/optional_logger",
        "logger launch_name should carry `/scoped` prefix: {logger:#?}"
    );
}
