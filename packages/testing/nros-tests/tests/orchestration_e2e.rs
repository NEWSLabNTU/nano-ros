//! Phase 211.A — in-tree orchestration foundation.
//!
//! Drives `nros plan` against the committed fixture under
//! `fixtures/orchestration_e2e/` and asserts the resulting `nros-plan.json`
//! carries the expected components / instances / callbacks. Decouples nano-ros
//! from `nros-cli`'s own test workspace so planner-side regressions are caught
//! in this tree.
//!
//! The launch file references a single `demo_pkg/talker` node; the fixture
//! ships a pre-collected `record.json` (output of `play_launch_parser`) so the
//! test does NOT depend on the parser binary being on `PATH`. The component's
//! source metadata is also pre-collected (`_metadata/talker.json` — a sidecar
//! preserved through the Phase 212.I migration since `nros migrate workspace`
//! deletes the per-pkg `src/<pkg>/metadata/` dir), so the plan stage stands
//! alone — the build stage is a separate 211 item.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/orchestration_e2e")
}

#[test]
fn orchestration_plan_emits_expected_entities() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let fixture = fixture_dir();
    let system_toml = fixture.join("demo_pkg_bringup/system.toml");
    assert!(
        system_toml.is_file(),
        "fixture missing demo_pkg_bringup/system.toml: {}",
        fixture.display()
    );
    let record = fixture.join("record.json");
    assert!(
        record.is_file(),
        "fixture missing committed record.json: {}",
        record.display()
    );
    let metadata = fixture.join("_metadata/talker.json");
    assert!(
        metadata.is_file(),
        "fixture missing _metadata/talker.json: {}",
        metadata.display()
    );

    let out = tempfile::tempdir().expect("tempdir");
    let status = Command::new(&nros)
        .arg("plan")
        .arg("demo_pkg")
        .arg("demo_pkg_bringup/launch/system.launch.xml")
        .arg("--workspace")
        .arg(&fixture)
        .arg("--nros-toml")
        .arg(&system_toml)
        .arg("--record")
        .arg(&record)
        .arg("--metadata")
        .arg(&metadata)
        .arg("--out-dir")
        .arg(out.path())
        .output()
        .expect("spawn nros plan");
    assert!(
        status.status.success(),
        "nros plan exit={} stderr={}",
        status.status,
        String::from_utf8_lossy(&status.stderr)
    );

    let plan_path = out.path().join("nros-plan.json");
    let plan: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&plan_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", plan_path.display())),
    )
    .expect("parse nros-plan.json");

    // Components — fixture's `system.toml` lists the `talker` component, so
    // the plan must surface it (matched against `_metadata/talker.json`).
    let components = plan["components"].as_array().expect("components array");
    assert!(
        components
            .iter()
            .any(|c| c["package"] == "demo_pkg" && c["component"] == "talker"),
        "missing demo_pkg::talker component in plan: {components:#?}"
    );

    // Instances — the launch file spawns one talker node; plan should mint a
    // single instance with the corresponding launch_name.
    let instances = plan["instances"].as_array().expect("instances array");
    let talker = instances
        .iter()
        .find(|i| {
            i["id"]
                .as_str()
                .is_some_and(|s| s.starts_with("demo_pkg.talker."))
        })
        .unwrap_or_else(|| panic!("no demo_pkg.talker.* instance in plan: {instances:#?}"));
    assert_eq!(
        talker["component"], "demo_pkg::talker",
        "instance component link"
    );
    assert_eq!(talker["executable"], "talker", "instance executable link");
    // launch_name carries either "/talker" (default arg) or the var override.
    assert!(
        talker["launch_name"]
            .as_str()
            .is_some_and(|s| s.contains("talker")),
        "talker launch_name unexpected: {:?}",
        talker["launch_name"]
    );

    // Callback groups — talker.json declares one `cb_timer`; planner should
    // bind it into a default group under the instance.
    let cb_groups = plan["callback_groups"].as_array().expect("callback_groups");
    assert!(
        cb_groups
            .iter()
            .any(|g| g["callbacks"].as_array().is_some_and(|cbs| cbs
                .iter()
                .any(|c| c.as_str() == Some("demo_pkg.talker.0/cb_timer")))),
        "cb_timer not bound into a callback group: {cb_groups:#?}"
    );

    // Build target — fixture pins zenoh + x86_64 native; plan must propagate.
    let build = &plan["build"];
    assert_eq!(build["rmw"], "zenoh", "rmw from system.toml");
    assert_eq!(
        build["target"], "x86_64-unknown-linux-gnu",
        "target from [deploy.native]"
    );
}
