//! Phase 211.J — `<include>` recursion safety + depth cap.
//!
//! Drives `nros plan` against `fixtures/orchestration_includes/` with three
//! pre-baked `record-{chain,cycle,deep}.json` files and gates each of
//! 211.J's behaviors:
//!
//! 1. **3-level chain** (`system → level_a → level_b → leaf`) — *works
//!    today*. Gated by `chain_3_levels_resolves_to_leaf`.
//!
//! 2. **Cyclic include** (`cycle_a → cycle_b → cycle_a → …`) — *planner
//!    silently returns an empty plan instead of raising a clean
//!    "cycle detected" error*. `play_launch_parser` swallows the cycle
//!    (dedupes by file path / hits an internal bound), so `record.json`
//!    has zero nodes and the planner emits zero instances. The 211.J
//!    bullet calls for a clean error. Gated by an `#[ignore]`-d test
//!    that flips on once the planner asserts a cycle diagnostic.
//!
//! 3. **17-level depth** (entry → lvl_0 → … → lvl_15 → lvl_16) —
//!    *no depth cap enforced*. 17 nested includes pass through and the
//!    leaf lands; 211.J proposes a default cap of 16. Gated by an
//!    `#[ignore]`-d test for the post-cap behavior.
//!
//! ## Fixture shape
//!
//! Launch files are committed for documentation (the `$(dirname)`
//! placeholders show the topology) but `play_launch_parser` doesn't
//! expand `$(dirname)` and can't resolve in-tree `$(find-pkg-share)`
//! either — so the committed `record-*.json` files (output of the
//! parser after `bake-records.sh` rewrites the placeholders to absolute
//! paths) are the actual test inputs. `--record` makes `nros plan`
//! ignore the launch path entirely, so the tests are portable.

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root().join("packages/testing/nros-tests/fixtures/orchestration_includes")
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
        .arg(format!("src/demo_inc/launch/{launch_name}"))
        .arg("--workspace")
        .arg(&fixture)
        .arg("--nros-toml")
        .arg(fixture.join("nros.toml"))
        .arg("--record")
        .arg(&record_path)
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

/// Phase 211.J follow-up — cyclic includes must error cleanly.
///
/// `play_launch_parser` swallows the cycle (`cycle_a → cycle_b → cycle_a
/// → …`) — `record.json` has zero nodes and the planner happily emits
/// zero instances. The 211.J bullet calls for a clean
/// `cycle-detected` diagnostic. Flip this test on once either the
/// parser or the planner raises the diagnostic; the fixture is already
/// shaped for it (`record-cycle.json` is the parser's empty-output
/// response to the cycle).
#[test]
#[ignore = "planner-side gap: cyclic include silently produces an empty plan, no cycle diagnostic"]
fn cycle_rejected_with_clear_diagnostic() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let (result, _plan) = plan("record-cycle.json", "cycle_entry.launch.xml");

    // The expected post-fix behavior: `nros plan` exits non-zero with a
    // stderr message mentioning "cycle" / "include" / the file path. The
    // current behavior is exit=0 + empty instances — caught by the
    // ignore-reason above.
    assert!(
        !result.status.success(),
        "nros plan should reject the cyclic include; exit={}",
        result.status
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("cycle") || stderr.contains("include"),
        "expected `cycle` / `include` diagnostic in stderr, got:\n{stderr}"
    );
}

/// Phase 211.J follow-up — depth cap enforcement.
///
/// The committed `record-deep.json` is the parser output for a 17-level
/// chain (`system → lvl_0 → … → lvl_15 → lvl_16` carrying the leaf
/// node) — one beyond the proposed default cap of 16. Today the planner
/// happily emits the leaf instance; the 211.J bullet calls for a
/// `depth-cap-exceeded` diagnostic. Flip this test on once the planner
/// enforces the cap.
#[test]
#[ignore = "planner-side gap: no depth cap on `<include>` nesting; 17 levels resolve silently"]
fn depth_cap_rejects_over_16() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let (result, _plan) = plan("record-deep.json", "deep_entry.launch.xml");

    // Expected post-fix: `nros plan` exits non-zero with a depth-cap
    // diagnostic. Current behavior is exit=0 + the leaf instance lands.
    assert!(
        !result.status.success(),
        "nros plan should reject the 17-level include chain (cap=16); exit={}",
        result.status
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("depth") || stderr.contains("cap") || stderr.contains("include"),
        "expected `depth` / `cap` / `include` diagnostic in stderr, got:\n{stderr}"
    );
}
