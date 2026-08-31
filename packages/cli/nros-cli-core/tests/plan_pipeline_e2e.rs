//! issue #202 — e2e for the LIVE plan pipeline (`nros metadata` → `nros plan`
//! → `nros check`) and the metadata-mode component build (`nros metadata
//! --build`). Salvaged from the retired `orchestration_e2e.rs`, whose other 15
//! tests exercised the standalone generated-system-package build path
//! (`orchestration::build::build_generated_package`) — a pipeline with no
//! production entry since phase-222 removed the `nros build`/`nros run` verbs
//! (retired together with this suite's repair; see archived issue 0202).
//!
//! The plan pipeline IS production: `nros plan`/`check`/`explain` are live CLI
//! verbs, `nros-build` (the `nros::main!` build.rs) consumes `NrosPlan` via
//! `planner::plan_system`, and `cmd::ws` renders bridge runtime config from
//! plans.
//!
//! On the no-compilation-in-tests rule (AGENTS.md Testing): the two
//! metadata-mode tests DO invoke cargo at runtime — deliberately. The verb
//! under test's contract is "discover + compile + run the component in
//! metadata mode to produce its source metadata"; a prebuilt fixture would
//! bypass exactly the compile-driver being tested. The probe crates are tiny
//! (seconds), and the suite runs in the host-side CLI lane, not a QEMU sweep.
//!
//! Old-suite path rot fixed here (the #202 root cause): the fixture helpers
//! counted `Path::ancestors()` from the crate's PRE-phase-218 location —
//! `codegen_root` landed on `packages/` (→ `packages/testing_workspaces/...`)
//! and `nano_ros_workspace` walked past the repo root entirely. The crate
//! lives at `packages/cli/nros-cli-core`, so the CLI sub-workspace root is
//! `ancestors().nth(1)` and the nano-ros repo root `ancestors().nth(3)`.
//!
//! Run with: `cargo test --manifest-path packages/cli/Cargo.toml --test plan_pipeline_e2e`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::{
    cmd::{check, metadata, plan},
    orchestration::{
        metadata_build::{MetadataBuildOptions, build_metadata},
        plan::NrosPlan,
        schema::ParameterValue,
        source_metadata::SourceMetadata,
    },
};
use serde_json::Value;

/// `nros metadata` (collect) → `nros plan` (model + manifest resolution) →
/// `nros check` (validation) over the in-tree fixture workspace, asserting the
/// canonical `nros-plan.json` + `record.json` shapes the live consumers
/// (`nros-build`, `nros explain`) read.
#[test]
fn fixture_workspace_plans_and_checks() {
    let fixture = fixture_workspace();
    let output = temp_output("plan_pipeline_e2e");
    let out_dir = output.join("build/e2e_system/nros");
    let demo_pkg = fixture.join("src/demo_pkg");

    metadata::run(metadata::Args {
        system_pkg: "e2e_system".to_string(),
        workspace: Some(fixture.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: vec![fixture.join("artifacts/talker.metadata.json")],
        build: false,
        nano_ros_workspace: None,
    })
    .expect("metadata command preserves fixture source metadata");

    // issue 0414 (tail) — this used to pass `demo_pkg` as a DIR and let
    // convention discovery find `demo_pkg/config/system_model.yaml`. phase-330
    // W4 made the SystemModel a build artifact and deleted every committed one,
    // so discovery had nothing to find and the test failed on a condition the
    // repo deliberately created. It now RESOLVES a model the way a build does,
    // which is also the stronger assertion: the resolver runs every time
    // instead of a file somebody generated once.
    let model = resolve_demo_pkg_model(&demo_pkg, &output);

    plan::run(plan::Args {
        nano_ros_path: None,
        system_pkg: "e2e_system".to_string(),
        // R-code.1 — the launch parse path is deleted; `plan` takes a resolved
        // model, so the dir argument is only the discovery root now.
        launch_file: demo_pkg.clone(),
        record: None,
        model: Some(model),
        file: None,
        exec: None,
        workspace: Some(fixture.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: Vec::new(),
        manifests: vec![demo_pkg.join("manifest/system.launch.yaml")],
        launch_args: Vec::new(),
        rmw: None,
        target: None,
    })
    .expect("plan command parses launch and writes checked artifacts");

    let plan_path = out_dir.join("nros-plan.json");
    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated plan");

    let plan: NrosPlan =
        serde_json::from_str(&fs::read_to_string(&plan_path).expect("read generated plan"))
            .expect("generated plan has canonical schema");
    assert_eq!(plan.system, "e2e_system");
    assert_eq!(plan.instances.len(), 1);
    assert_eq!(plan.instances[0].package, "demo_pkg");
    assert_eq!(plan.instances[0].parameters[0].name, "rate_hz");
    assert_eq!(
        plan.instances[0].parameters[0].value,
        ParameterValue::Integer(25)
    );

    let record: Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("record.json")).expect("read record"),
    )
    .expect("record is JSON");
    let nodes = record["node"].as_array().expect("record has node array");
    assert_eq!(nodes[0]["package"].as_str(), Some("demo_pkg"));
    assert_eq!(nodes[0]["executable"].as_str(), Some("talker"));

    let _ = fs::remove_dir_all(&output);
}

/// Metadata-mode component build: compile + run the fixture talker in
/// metadata mode and assert the emitted `SourceMetadata` shape.
#[test]
fn metadata_mode_build_emits_source_metadata_for_component() {
    let fixture = fixture_workspace();
    let out = temp_output("metadata_build");
    let output_path = out.join("talker.metadata.json");

    build_metadata(&MetadataBuildOptions {
        component_id: "demo_pkg::talker".to_string(),
        class: None,
        crate_name: None,
        package: "demo_pkg".to_string(),
        component: "talker".to_string(),
        executable: Some("talker".to_string()),
        exported_symbol: Some("nros_component_talker".to_string()),
        component_dir: fixture.join("src/demo_pkg"),
        nano_ros_workspace: nano_ros_workspace(),
        output_path: output_path.clone(),
        harness_dir: out.join("probe"),
    })
    .expect("metadata-mode build produces source metadata");

    let raw = fs::read_to_string(&output_path).expect("read produced metadata");
    let meta: SourceMetadata = serde_json::from_str(&raw).expect("valid SourceMetadata JSON");
    assert_eq!(meta.package, "demo_pkg");
    assert_eq!(meta.component, "talker");
    assert_eq!(meta.nodes.len(), 1);
    // Entity ids are the component's OWN registration names (the recorder
    // stopped synthesizing `node_`/`pub_`/`timer_` prefixes while this suite
    // was dead — assertions track the live emitter).
    let node = &meta.nodes[0];
    assert_eq!(node.id, "talker");
    assert_eq!(node.publishers.len(), 1);
    assert_eq!(node.publishers[0].id, "chatter");
    assert_eq!(node.timers.len(), 1);
    assert_eq!(node.timers[0].id, "cb_timer");

    let _ = fs::remove_dir_all(&out);
}

/// Phase 172.E CLI wiring — `nros metadata --build` discovers the declared
/// `probe_pkg` component (via its `component_nros.toml`), compiles + runs it in
/// metadata mode to produce the missing `source-metadata`, then collects it.
#[test]
fn metadata_build_discovers_missing_sources() {
    let ws = cli_root().join("testing_workspaces/metadata_build_ws");
    let produced = ws.join("src/probe_pkg/node.metadata.json");
    let _ = fs::remove_file(&produced); // start clean (gitignored)
    let out = temp_output("metadata_build_discovery");

    metadata::run(metadata::Args {
        system_pkg: "probe_sys".to_string(),
        workspace: Some(ws.clone()),
        out_dir: Some(out.clone()),
        metadata: Vec::new(),
        build: true,
        nano_ros_workspace: Some(nano_ros_workspace()),
    })
    .expect("nros metadata --build discovers + produces missing source metadata");

    // The component's declared source-metadata path was produced...
    assert!(
        produced.is_file(),
        "metadata-mode build wrote {}",
        produced.display()
    );
    let meta: SourceMetadata =
        serde_json::from_str(&fs::read_to_string(&produced).expect("read produced"))
            .expect("valid SourceMetadata");
    assert_eq!(meta.package, "probe_pkg");
    assert_eq!(meta.nodes.len(), 1);
    // Live-emitter ids (see the sibling test's note on the prefix change).
    assert_eq!(meta.nodes[0].id, "probe");
    assert_eq!(meta.nodes[0].timers.len(), 1);
    // ...and collected into the out metadata dir.
    assert!(out.join("metadata/node.metadata.json").is_file());

    let _ = fs::remove_file(&produced); // don't leave it in the source tree
    let _ = fs::remove_dir_all(&out);
}

/// The in-tree fixture workspace (owned by the CLI sub-workspace since the
/// phase-218 in-tree move — `packages/cli/testing_workspaces/`).
/// Resolve `demo_pkg`'s launch file into a SystemModel under `out`, and return
/// its path.
///
/// The SystemModel is a build ARTIFACT (phase-330 W4 / RFC-0063), so a test
/// that needs one produces it exactly as the build does: run the
/// `nros-launch-resolve` helper by ABSOLUTE path (never `$PATH` — issue 0285)
/// over the bringup's launch XML. Skips loudly rather than silently passing if
/// the resolver was never built, since a missing resolver and a broken plan
/// look identical from the assertion below.
fn resolve_demo_pkg_model(demo_pkg: &Path, out: &Path) -> PathBuf {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let resolver = repo.join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    assert!(
        resolver.is_file(),
        "nros-launch-resolve not built at {} — run `just setup-launch-resolve`",
        resolver.display()
    );
    let model = out.join("system_model.yaml");
    fs::create_dir_all(out).expect("create model out dir");
    let output = std::process::Command::new(&resolver)
        .arg(demo_pkg.join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(demo_pkg)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for the e2e fixture bringup:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    model
}

fn fixture_workspace() -> PathBuf {
    cli_root().join("testing_workspaces/orchestration_e2e")
}

/// `packages/cli/` — the CLI sub-workspace root (this crate lives at
/// `packages/cli/nros-cli-core`).
fn cli_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(1)
        .expect("cli sub-workspace root ancestor")
        .to_path_buf()
}

/// The nano-ros repo root (`packages/cli/nros-cli-core` → up three).
fn nano_ros_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("nano-ros workspace ancestor")
        .to_path_buf()
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
/// files live in `$project/tmp/`, not the system temp dir).
fn temp_output(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = nano_ros_workspace()
        .join("tmp")
        .join(format!("{name}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}
