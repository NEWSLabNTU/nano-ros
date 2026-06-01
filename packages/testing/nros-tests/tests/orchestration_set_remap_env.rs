//! Phase 211.E — `<set_remap>` / `<set_env>` / `<executable>` deploy-time
//! scoping.
//!
//! Drives `nros plan` against `fixtures/orchestration_set_remap_env/` and
//! gates each of 211.E's three sub-bullets:
//!
//! 1. **`<set_remap>` scoping** — *works today*. A
//!    `<group><set_remap from=in to=/scoped/in><node><node></group>`
//!    causes each child node's `instances[*].remaps` to carry the scoped
//!    pair, AND the child's subscriber `resolved_name` resolves to
//!    `/scoped/in`. Gated by `set_remap_propagates_to_group_children`.
//!
//! 2. **`<set_env>` scoping** — *planner-side gap*. `play_launch_parser`
//!    surfaces the env entry in `record.json` (`node.env = [["DEMO_LEVEL",
//!    "verbose"]]`), but the planner doesn't thread it onto
//!    `instances[*].env` — that field stays `None`. Gated by an
//!    `#[ignore]`-d test that will flip on once the planner change lands.
//!
//! 3. **`<executable>` as a spawn entity** — *planner-side gap*.
//!    `play_launch_parser` records `<executable>` as a `record.json` `node`
//!    with `package=None`; the planner then errors `missing-package`
//!    rather than emitting a non-rmw spawn entity. Not exercised by the
//!    committed fixture (adding it blocks the rest of the plan stage);
//!    captured as a phase-doc bullet + `#[ignore]` gate referencing a
//!    second fixture that would land alongside the planner change.
//!
//! ## Pre-baked record
//!
//! `record.json` (committed) is the output of `play_launch_parser` against
//! the fixture's launch. Decouples the test from the parser binary on PATH
//! (same pattern as 211.A/B/D).

use std::{path::PathBuf, process::Command};

fn fixture_dir() -> PathBuf {
    nros_tests::project_root()
        .join("packages/testing/nros-tests/fixtures/orchestration_set_remap_env")
}

fn run_plan() -> serde_json::Value {
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
        .arg("demo_se")
        .arg("demo_se_bringup/launch/system.launch.xml")
        .arg("--workspace")
        .arg(&fixture)
        .arg("--nros-toml")
        .arg(fixture.join("demo_se_bringup/system.toml"))
        .arg("--record")
        .arg(&record)
        .arg("--metadata")
        .arg(fixture.join("_metadata/worker_a.json"))
        .arg("--metadata")
        .arg(fixture.join("_metadata/worker_b.json"))
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

    serde_json::from_str(
        &std::fs::read_to_string(out.path().join("nros-plan.json")).expect("read plan"),
    )
    .expect("parse plan")
}

#[test]
fn set_remap_propagates_to_group_children() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let plan = run_plan();
    let instances = plan["instances"].as_array().expect("instances");

    assert_eq!(
        instances.len(),
        2,
        "expected worker_a + worker_b instances: {instances:#?}"
    );

    for needle in ["worker_a", "worker_b"] {
        let inst = instances
            .iter()
            .find(|i| i["component"] == format!("demo_se::{needle}"))
            .unwrap_or_else(|| panic!("no demo_se::{needle} instance: {instances:#?}"));

        // 1. The set_remap pair must land on each child's `remaps` block.
        let remaps = inst["remaps"].as_array().expect("remaps array");
        assert!(
            remaps
                .iter()
                .any(|r| r["from"] == "in" && r["to"] == "/scoped/in"),
            "{needle}: set_remap pair did not propagate into instance.remaps: {remaps:#?}"
        );

        // 2. The subscriber's resolved topic name must reflect the remap —
        //    otherwise the worker would actually subscribe to `/in`, not
        //    `/scoped/in`, defeating the whole point of the scoping.
        let sub = inst["nodes"][0]["entities"]
            .as_array()
            .expect("entities")
            .iter()
            .find(|e| e["role"] == "subscriber")
            .unwrap_or_else(|| panic!("{needle}: no subscriber entity"));
        assert_eq!(
            sub["resolved_name"], "/scoped/in",
            "{needle}: subscriber resolved_name did not pick up the set_remap target: {sub:#?}"
        );
    }
}

/// Phase 211.E — `<set_env>` declarations land on `instances[*].env`.
///
/// Resolved upstream in `nros-cli` planner commit `0b78ab8` (Phase 211.E):
/// the parser already records `node.env = [["DEMO_LEVEL", "verbose"]]`;
/// the planner now threads each pair through to the public schema as
/// `instances[*].env: [{name, value}, …]`. The deploy stage then has
/// something to ship onto the spawned process.
#[test]
fn set_env_propagates_to_group_children() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let plan = run_plan();
    let instances = plan["instances"].as_array().expect("instances");

    for needle in ["worker_a", "worker_b"] {
        let inst = instances
            .iter()
            .find(|i| i["component"] == format!("demo_se::{needle}"))
            .unwrap_or_else(|| panic!("no demo_se::{needle} instance"));
        let env = inst["env"].as_array().unwrap_or_else(|| {
            panic!(
                "{needle}: instances[*].env is not an array (refreshed nros CLI not on PATH?): {inst:#?}"
            )
        });
        assert!(
            env.iter()
                .any(|kv| kv["name"] == "DEMO_LEVEL" && kv["value"] == "verbose"),
            "{needle}: set_env entry did not propagate into instance.env: {env:#?}"
        );
    }
}

/// Phase 211.E — `<executable>` declarations land on
/// `plan.executables[*]` as non-rmw spawn entries.
///
/// Resolved upstream in `nros-cli` planner commit `4ad1ae8` (Phase 211.E):
/// the parser writes `<executable cmd="…">` as a `record.node` with
/// `package=None`; the planner used to reject those as `missing-package`,
/// but now emits a dedicated `PlanExecutable` for each. The fixture's
/// launch carries a `<executable cmd="/bin/echo" name="greeter">` with
/// two `<arg>` children, inheriting the group's `<set_env>` —
/// covers cmd/args/env propagation in one go.
#[test]
fn executable_emits_spawn_entity() {
    if !nros_tests::require_nros_cli() {
        nros_tests::skip!("nros CLI not found");
    }
    let plan = run_plan();
    let execs = plan["executables"].as_array().unwrap_or_else(|| {
        panic!(
            "plan must surface an `executables` array (refreshed nros CLI not on PATH?): {plan:#?}"
        )
    });
    assert_eq!(
        execs.len(),
        1,
        "expected exactly one <executable> entry, got: {execs:#?}"
    );
    let exec = &execs[0];
    assert_eq!(exec["id"], "executable.greeter.0");
    assert_eq!(exec["name"], "greeter");
    assert_eq!(exec["namespace"], "/");
    assert_eq!(
        exec["cmd"],
        serde_json::json!(["/bin/echo", "hello", "from-launch"]),
        "cmd must carry the fully-resolved argv (executable + args)"
    );
    assert_eq!(
        exec["args"],
        serde_json::json!(["hello", "from-launch"]),
        "args must carry the `<arg>` children separately"
    );
    // The executable sits OUTSIDE the `<group>` carrying `<set_env>` but
    // ROS launch's set_env semantics are "global once declared" — every
    // sibling entity past the declaration inherits the env. The parser
    // already enforces that; the planner forwards it.
    assert_eq!(
        exec["env"],
        serde_json::json!([{"name": "DEMO_LEVEL", "value": "verbose"}]),
        "env from the preceding <set_env> must propagate onto the executable"
    );
    assert_eq!(
        exec["trace"]["launch_record_entity"],
        "record://executable.greeter.0"
    );
}
