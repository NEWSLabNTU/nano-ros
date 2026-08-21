//! Phase 211.J — `<include>` recursion: the 3-level chain walk.
//!
//! Drives `nros plan` against `fixtures/orchestration_includes/` with a
//! pre-baked `record-chain.json` and gates the 3-level chain
//! (`system → level_a → level_b → leaf`) via `chain_3_levels_resolves_to_leaf`.
//! `--record` makes `nros plan` ignore the launch path entirely, so the test
//! is portable and needs no `play_launch_parser`.
//!
//! **Cycle-detection and include depth-cap enforcement moved to
//! `ros-launch-resolve`** (RFC-0060 layer 2 owns include expansion since
//! phase-296; `nros plan` no longer parses launch XML). The former
//! `cycle_rejected_with_clear_diagnostic` and `depth_cap_rejects_over_16`
//! tests drove the deleted parse path through `nros plan` and were removed
//! (issue 0381); those behaviors are tested in the resolver's own repo.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::fixtures::fixture_dir("orchestration_includes")
}

fn plan(record_name: &str, launch_name: &str) -> (std::process::Output, Option<serde_json::Value>) {
    let nros = nros_tests::nros_cli_bin_path().expect("require_nros_cli passed");
    let fixture = fixture_dir();
    let record_path = fixture.join(record_name);
    assert!(
        record_path.is_file(),
        "fixture missing committed {record_name}: {}",
        record_path.display()
    );

    let out = tempfile::tempdir().expect("tempdir");
    let result = Command::new(&nros)
        .arg("plan")
        .arg("demo_inc")
        .arg(format!("demo_inc_bringup/launch/{launch_name}"))
        .arg("--workspace")
        .arg(&fixture)
        .arg("--record")
        .arg(&record_path)
        .arg("--metadata")
        .arg(fixture.join("_metadata/leaf.json"))
        .arg("--out-dir")
        .arg(out.path())
        .output()
        .expect("spawn nros plan");

    let plan_path = out.path().join("nros-plan.json");
    let parsed = if plan_path.is_file() {
        Some(
            serde_json::from_str(&std::fs::read_to_string(&plan_path).expect("read plan"))
                .expect("parse plan"),
        )
    } else {
        None
    };
    (result, parsed)
}

#[test]
fn chain_3_levels_resolves_to_leaf() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let (result, plan) = plan("record-chain.json", "system.launch.xml");
    assert!(
        result.status.success(),
        "nros plan exit={} stderr={}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
    let plan = plan.expect("plan json");
    let instances = plan["instances"].as_array().expect("instances array");

    // The leaf node is buried 3 includes deep (system → level_a → level_b
    // → leaf). The planner must walk every include and surface the leaf
    // node, otherwise basic launch composition is broken.
    assert_eq!(
        instances.len(),
        1,
        "expected exactly one leaf instance after 3-level include walk: {instances:#?}"
    );
    let leaf = &instances[0];
    assert_eq!(leaf["component"], "demo_inc::leaf", "wrong component");
    assert_eq!(leaf["launch_name"], "/leaf_node", "wrong launch_name");
}
