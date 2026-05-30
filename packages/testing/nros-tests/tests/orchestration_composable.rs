//! Phase 211.B — composable-container planner shape.
//!
//! Drives `nros plan` against the committed `fixtures/orchestration_composable/`
//! workspace (one `<node_container>` hosting two `<composable_node>` children:
//! a `Talker` + `Listener` sharing the remapped `/chatter_a` topic) and asserts
//! the resulting `nros-plan.json` shape.
//!
//! ## What this test gates
//!
//! 1. **Current planner shape (regression baseline).** The planner today
//!    emits each `<composable_node>` as a *flat* `instances[*]` entry —
//!    there is no `container_id` field grouping them under their parent
//!    `<node_container>`. Production ROS (Nav2, Autoware, MoveIt) **relies**
//!    on the container hosting many nodes in one process for intra-process
//!    zero-copy; nano-ros doesn't yet model that grouping in the plan.
//!
//! 2. **Per-child fidelity.** Even without grouping, the planner must
//!    propagate per-composable parameter overrides, remappings, and the
//!    component / metadata cross-reference correctly. The test pins each.
//!
//! 3. **The 211.B planner-fix landing site.** When `nros-cli` learns to
//!    emit `entities[*].container_id` + `entities[*].kind`, the post-fix
//!    block (currently a `TODO`) flips to asserting the new shape. The
//!    fixture stays valid; only the test's expected-shape changes.
//!
//! ## What this test does NOT gate
//!
//! - The actual one-process-many-nodes runtime path. `Executor::open` +
//!   `create_node(name)` already supports N nodes per process (Phase 172
//!   W.5); a build-stage e2e that loads two composable libraries into
//!   one process is part of 211.B's runtime sub-item, not 211.A's
//!   plan-stage foundation.

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
        .arg("src/demo_container/launch/system.launch.xml")
        .arg("--workspace")
        .arg(&fixture)
        .arg("--nros-toml")
        .arg(fixture.join("nros.toml"))
        .arg("--record")
        .arg(&record)
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

    // ─── Current planner shape (regression baseline) ────────────────────────
    //
    // Two flat `instances[*]` entries — one per `<composable_node>`. The
    // parent `<node_container>` is NOT emitted as its own instance today
    // (it surfaces only as a phantom auto-synthesized `components[*]`
    // entry referencing an in-out-dir metadata stub).
    assert_eq!(
        instances.len(),
        2,
        "expected 2 flat composable instances today, got {}: {instances:#?}",
        instances.len()
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

    // Neither composable carries a `container_id` field yet — that's the
    // 211.B planner gap. When the planner learns the grouping, flip the
    // `is_null` assertions below to `==` against the parent container id.
    assert!(
        talker.get("container_id").is_none_or(|v| v.is_null()),
        "talker.container_id present (planner-fix landed? update assertions): {talker:#?}"
    );
    assert!(
        listener.get("container_id").is_none_or(|v| v.is_null()),
        "listener.container_id present (planner-fix landed? update assertions): {listener:#?}"
    );

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

    // Components index — must carry an entry per composable class. The
    // parent container's `exec="container"` ALSO appears as a phantom
    // auto-synthesized component today (planner fills in metadata it can't
    // find on disk); pinning the count would over-constrain a planner-fix
    // that drops the phantom, so only assert the two real ones exist.
    let components = plan["components"].as_array().expect("components array");
    for needle in ["Talker", "Listener"] {
        assert!(
            components
                .iter()
                .any(|c| c["component"] == needle && c["package"] == "demo_container"),
            "components missing demo_container::{needle}: {components:#?}"
        );
    }

    // ─── TODO 211.B planner-fix expected shape ─────────────────────────────
    //
    // Once `nros-cli` groups composables under their `<node_container>`,
    // this block flips on. Pseudocode:
    //
    //     let container = instances.iter().find(|i| i["kind"] == "container")
    //         .expect("container instance");
    //     for child in [talker, listener] {
    //         assert_eq!(child["kind"], "composable_node");
    //         assert_eq!(child["container_id"], container["id"]);
    //     }
    //
    // Leave as a comment until the planner change lands.
}
