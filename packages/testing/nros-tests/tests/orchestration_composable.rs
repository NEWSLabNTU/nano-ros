//! Phase 211.B — composable-container planner shape.
//!
//! Drives `nros plan` against the committed `fixtures/orchestration_composable/`
//! workspace (one `<node_container>` hosting two `<composable_node>` children:
//! a `Talker` + `Listener` sharing the remapped `/chatter_a` topic) and asserts
//! the resulting `nros-plan.json` shape.
//!
//! ## What this test gates
//!
//! 1. **Container + composable grouping.** Resolved upstream in `nros-cli`
//!    planner `706023c`: the planner reads `record.container`, mints a
//!    container instance with `kind = "container"`, and resolves each
//!    `<composable_node>` child's `target_container_name` to the parent's
//!    instance id (`container_id` field on the child + `kind =
//!    "composable_node"`). Production ROS launches (Nav2, Autoware, MoveIt)
//!    rely on this single-process composable container model for
//!    intra-process zero-copy.
//!
//! 2. **Per-child fidelity.** Each composable still propagates its own
//!    parameter overrides, remappings, and component / metadata
//!    cross-reference.
//!
//! ## What this test does NOT gate
//!
//! - The actual one-process-many-nodes runtime path. `Executor::open` +
//!   `create_node(name)` already supports N nodes per process (Phase 172
//!   W.5); a build-stage e2e that loads two composable libraries into
//!   one container binary is part of 211.B's runtime sub-item.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/orchestration_composable")
}

#[test]
fn composable_container_plan_shape() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let fixture = fixture_dir();
    let record = fixture.join("record.json");
    assert!(
        record.is_file(),
        "fixture missing committed record.json: {}",
        record.display()
    );

    let out = tempfile::tempdir().expect("tempdir");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_container")
        .arg("demo_container_bringup/launch/system.launch.xml")
        .arg("--workspace")
        .arg(&fixture)
        .arg("--record")
        .arg(&record)
        .arg("--metadata")
        .arg(fixture.join("_metadata/talker.json"))
        .arg("--metadata")
        .arg(fixture.join("_metadata/listener.json"))
        .arg("--out-dir")
        .arg(out.path())
        .output()
        .expect("spawn nros plan");
    assert!(
        result.status.success(),
        "nros plan exit={} stderr={}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );

    let plan: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.path().join("nros-plan.json")).expect("read plan"),
    )
    .expect("parse plan");

    let instances = plan["instances"].as_array().expect("instances array");

    // ─── Container + composable grouping ───────────────────────────────────
    //
    // 3 instances: the `<node_container>` (kind=container) plus the two
    // `<composable_node>` children (kind=composable_node, container_id =
    // parent's instance id). Phase 211.B planner change (nros-cli 706023c).
    assert_eq!(
        instances.len(),
        3,
        "expected container + 2 composables, got {}: {instances:#?}",
        instances.len()
    );

    let container = instances
        .iter()
        .find(|i| i["kind"] == "container")
        .unwrap_or_else(|| panic!("no container instance: {instances:#?}"));
    let container_id = container["id"]
        .as_str()
        .expect("container id is a string")
        .to_string();
    assert!(
        container.get("container_id").is_none_or(|v| v.is_null()),
        "container itself must NOT carry container_id: {container:#?}"
    );

    let by_component = |needle: &str| -> &serde_json::Value {
        instances
            .iter()
            .find(|i| {
                i["component"]
                    .as_str()
                    .is_some_and(|s| s == format!("demo_container::{needle}"))
            })
            .unwrap_or_else(|| panic!("no {needle} instance: {instances:#?}"))
    };
    let talker = by_component("Talker");
    let listener = by_component("Listener");

    for (label, inst) in [("Talker", talker), ("Listener", listener)] {
        assert_eq!(
            inst["kind"], "composable_node",
            "{label}: kind must be composable_node"
        );
        assert_eq!(
            inst["container_id"], container_id,
            "{label}: container_id must point at the parent container"
        );
    }

    // ─── Per-child fidelity ─────────────────────────────────────────────────
    //
    // Talker: rate_hz override from `<param>` = 20 (not the metadata default
    // of 10) — exercises composable-child parameter propagation.
    let talker_params = talker["parameters"].as_array().expect("params array");
    let rate = talker_params
        .iter()
        .find(|p| p["name"] == "rate_hz")
        .expect("rate_hz param");
    assert_eq!(
        rate["value"], 20,
        "rate_hz override from <param> did not propagate: {rate:#?}"
    );

    // Both composables remap `chatter` → `/chatter_a`; the resolved
    // publisher / subscriber topic must reflect the remap so they
    // actually connect on the same topic.
    let pub_entity = talker["nodes"][0]["entities"]
        .as_array()
        .expect("talker entities")
        .iter()
        .find(|e| e["role"] == "publisher")
        .expect("publisher entity");
    assert_eq!(
        pub_entity["resolved_name"], "/chatter_a",
        "talker publisher remap missing"
    );
    let sub_entity = listener["nodes"][0]["entities"]
        .as_array()
        .expect("listener entities")
        .iter()
        .find(|e| e["role"] == "subscriber")
        .expect("subscriber entity");
    assert_eq!(
        sub_entity["resolved_name"], "/chatter_a",
        "listener subscriber remap missing"
    );

    // Components index — carries an entry per composable class. The
    // container's auto-synthesized component entry may or may not appear
    // depending on metadata discovery; pinning the count would over-
    // constrain, so only assert the two composable classes exist.
    let components = plan["components"].as_array().expect("components array");
    for needle in ["Talker", "Listener"] {
        assert!(
            components
                .iter()
                .any(|c| c["component"] == needle && c["package"] == "demo_container"),
            "components missing demo_container::{needle}: {components:#?}"
        );
    }
}
