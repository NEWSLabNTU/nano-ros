//! Phase 212.M-F.17 — α-bridge integration test.
//!
//! Validates that `plan_system` configures successfully against a workspace
//! whose component package declares ONLY a
//! `[package.metadata.nros.component]` table in its `Cargo.toml` — no
//! sidecar `metadata/*.json`. Prior to M-F.17 this configuration tripped
//! the `missing-source-metadata` diagnostic because the planner's
//! `find_source_metadata` walked only the file-artifact slice.
//!
//! The α-bridge synthesises a minimal `JsonArtifact` per component
//! metadata table at workspace-discovery time; this test pins the
//! end-to-end shape:
//!
//! * `plan_system` returns Ok (no errors).
//! * Zero diagnostics on the resulting plan.
//! * The emitted `nros-plan.json` carries one `components[]` entry whose
//!   `synthetic` provenance marker is set (the synth artifact landed in
//!   the planner's metadata slice and survived dedup).

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::orchestration::planner::{PlanOptions, plan_system};
use serde_json::Value;

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "phase-212-mf17-{tag}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Materialise the canonical M-F.17 fixture: a single workspace member
/// `talker_pkg` that declares its component via
/// `[package.metadata.nros.component]` in `Cargo.toml`, accompanied by
/// the minimum `package.xml` + `nros.toml` (bringup) the discovery walk
/// expects. NO sidecar `metadata/*.json` — that's the whole point.
fn write_fixture(root: &Path) {
    // Workspace top-level — `package.xml` + a `nros.toml` so the planner
    // routes through the bringup discovery path. The system_pkg name on
    // PlanOptions points at the workspace root itself.
    fs::write(
        root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3"><name>cargo_self_bringup</name><version>0.0.0</version>
<description>M-F.17 fixture</description>
<maintainer email="a@b.c">a</maintainer><license>MIT</license>
</package>"#,
    )
    .unwrap();
    fs::write(
        root.join("nros.toml"),
        r#"
[system]
name = "cargo_self_bringup"
rmw = "zenoh"
domain_id = 0
"#,
    )
    .unwrap();

    // Workspace member: `talker_pkg` with `[package.metadata.nros.component]`
    // ONLY — no `metadata/*.json` sidecar.
    let pkg = root.join("src/talker_pkg");
    fs::create_dir_all(pkg.join("src")).unwrap();
    fs::write(
        pkg.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3"><name>talker_pkg</name><version>0.0.0</version>
<description>talker</description>
<maintainer email="a@b.c">a</maintainer><license>MIT</license>
</package>"#,
    )
    .unwrap();
    fs::write(
        pkg.join("Cargo.toml"),
        r#"
[package]
name = "talker_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "talker_pkg::Talker"
name = "talker"
default_namespace = "/demo"
"#,
    )
    .unwrap();
    fs::write(pkg.join("src/lib.rs"), "").unwrap();
}

/// Write a precomputed `record.json` that names the same
/// `(package, executable)` pair the α-bridge synthesises from
/// `Cargo.toml`. Avoids depending on the external `play_launch_parser`
/// binary — that path is exercised by `orchestration_e2e`.
fn write_record(root: &Path) -> PathBuf {
    let record = serde_json::json!({
        "node": [{
            "package": "talker_pkg",
            "executable": "talker_pkg",
            "name": "talker",
            "namespace": "/demo"
        }]
    });
    let path = root.join("record.json");
    fs::write(&path, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    path
}

#[test]
fn plan_system_succeeds_with_cargo_metadata_alpha_bridge() {
    let root = temp_root("self_bringup");
    write_fixture(&root);
    let record_path = write_record(&root);
    let launch_file = root.join("launch.placeholder");
    // `record_file` is provided, so the planner never reads `launch_file`.
    // Touch the file anyway so any future "path must exist" assertion
    // doesn't trip on this test.
    fs::write(&launch_file, "").unwrap();

    let out_root = root.join("build/cargo_self_bringup/nros");
    let output = plan_system(PlanOptions {
        system_pkg: "cargo_self_bringup".to_string(),
        workspace_root: root.clone(),
        launch_file,
        record_file: Some(record_path),
        out_root: out_root.clone(),
        metadata_files: Vec::new(),
        manifest_files: Vec::new(),
        nros_toml_files: Vec::new(),
        launch_args: Vec::new(),
        rmw: None,
    })
    .expect("plan_system configures against cargo-metadata-only workspace");

    let plan: Value =
        serde_json::from_str(&fs::read_to_string(&output.plan_path).unwrap()).unwrap();

    // No `missing-source-metadata` diagnostic.
    let diagnostics = plan
        .get("diagnostics")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    assert!(
        diagnostics.is_empty(),
        "diagnostics must be empty (α-bridge cleared missing-source-metadata): {diagnostics:?}"
    );

    // One component, derived from the cargo metadata α-bridge.
    let components = plan["components"].as_array().expect("components array");
    assert_eq!(components.len(), 1, "one component synthesised");
    let component = &components[0];
    assert_eq!(component["package"], "talker_pkg");
    assert_eq!(component["component"], "talker");
    assert_eq!(component["language"], "rust");

    // The synthetic provenance survives onto the planner's metadata slice.
    // `schema_components` records the artifact's `source_metadata` path
    // (the source Cargo.toml when α-bridge supplied it). Verify the path
    // points at the talker_pkg Cargo.toml.
    let source_metadata = component["source_metadata"]
        .as_str()
        .expect("source_metadata recorded");
    assert!(
        source_metadata.ends_with("Cargo.toml"),
        "α-bridge artifact path is the source Cargo.toml: {source_metadata}"
    );

    // The preserved-to-disk metadata dir should NOT contain a `Cargo.toml`
    // (synthetic artifacts are skipped in `preserve_metadata`).
    let preserved = out_root.join("metadata");
    if preserved.is_dir() {
        for entry in fs::read_dir(&preserved).unwrap() {
            let path = entry.unwrap().path();
            assert!(
                !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap()
                    .ends_with("Cargo.toml"),
                "synthetic Cargo.toml must not be preserved as a metadata file: {}",
                path.display()
            );
        }
    }
}

/// Verifies system planning still reports missing metadata for a pure empty package.
#[test]
fn plan_system_reports_missing_metadata() {
    // Inverse case: a workspace member that has NEITHER a sidecar
    // metadata JSON NOR a cargo metadata table still trips the
    // `missing-source-metadata` diagnostic. The α-bridge is additive —
    // packages that supply nothing remain unplannable, same as pre-M-F.17.
    let root = temp_root("pure_empty");
    fs::write(
        root.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3"><name>pure_empty_bringup</name><version>0.0.0</version>
<description>x</description><maintainer email="a@b.c">a</maintainer><license>MIT</license>
</package>"#,
    )
    .unwrap();
    fs::write(
        root.join("nros.toml"),
        r#"
[system]
name = "pure_empty_bringup"
rmw = "zenoh"
domain_id = 0
"#,
    )
    .unwrap();
    // Empty member package — no Cargo.toml, no metadata JSON.
    let pkg = root.join("src/empty_pkg");
    fs::create_dir_all(&pkg).unwrap();
    fs::write(
        pkg.join("package.xml"),
        r#"<?xml version="1.0"?>
<package format="3"><name>empty_pkg</name><version>0.0.0</version>
<description>x</description><maintainer email="a@b.c">a</maintainer><license>MIT</license>
</package>"#,
    )
    .unwrap();

    let record = serde_json::json!({
        "node": [{
            "package": "empty_pkg",
            "executable": "empty_pkg",
            "name": "empty"
        }]
    });
    let record_path = root.join("record.json");
    fs::write(&record_path, serde_json::to_string_pretty(&record).unwrap()).unwrap();
    let launch_file = root.join("launch.placeholder");
    fs::write(&launch_file, "").unwrap();

    let err = plan_system(PlanOptions {
        system_pkg: "pure_empty_bringup".to_string(),
        workspace_root: root.clone(),
        launch_file,
        record_file: Some(record_path),
        out_root: root.join("build/pure_empty_bringup/nros"),
        metadata_files: Vec::new(),
        manifest_files: Vec::new(),
        nros_toml_files: Vec::new(),
        launch_args: Vec::new(),
        rmw: None,
    })
    .expect_err("plan_system rejects pkg with no metadata source");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("missing-source-metadata") || msg.contains("missing source metadata"),
        "diagnostic surfaces the missing metadata: {msg}"
    );
}
