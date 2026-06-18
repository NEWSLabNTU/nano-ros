use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::{
    cmd::{check, explain, metadata, plan},
    orchestration::plan::NrosPlan,
};

#[test]
fn orchestration_metadata_plan_check_commands_share_artifacts() {
    let root = temp_workspace("metadata_plan_check_commands");
    let out_dir = root.join("build/system_pkg/nros");
    write_workspace_fixture(&root);

    metadata::run(metadata::Args {
        system_pkg: "system_pkg".to_string(),
        workspace: Some(root.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: vec![root.join("talker.metadata.json")],
        build: false,
        nano_ros_workspace: None,
    })
    .expect("metadata command preserves source metadata");

    let preserved_metadata = out_dir.join("metadata/talker.metadata.json");
    assert!(preserved_metadata.is_file());

    plan::run(plan::Args {
        system_pkg: "system_pkg".to_string(),
        launch_file: root.join("system.launch.xml"),
        record: Some(root.join("record.json")),
        file: None,
        exec: None,
        workspace: Some(root.clone()),
        out_dir: Some(out_dir.clone()),
        metadata: Vec::new(),
        manifests: vec![root.join("manifest.launch.yaml")],
        launch_args: Vec::new(),
        rmw: None,
        target: None,
    })
    .expect("plan command consumes preserved metadata");

    let plan_path = out_dir.join("nros-plan.json");
    check::run(check::Args {
        plan: plan_path.clone(),
        package_xml_drift: Vec::new(),
        bringup: false,
        workspace: None,
    })
    .expect("check command validates generated plan");

    let plan: NrosPlan =
        serde_json::from_str(&fs::read_to_string(plan_path).expect("read generated plan"))
            .expect("generated plan has canonical schema");
    assert_eq!(plan.system, "system_pkg");
    assert_eq!(plan.instances.len(), 1);
    assert_eq!(plan.instances[0].callbacks.len(), 1);
    assert_eq!(plan.instances[0].sched_bindings.len(), 1);
    assert_eq!(plan.instances[0].parameters[0].name, "rate_hz");

    // `explain` renders the same artifact readably (Phase 172.F). Capture the
    // rendering and assert it surfaces the plan's structure — system header,
    // the launch-instance→component map, a node, the parameter + its source,
    // and the callback→context binding.
    let mut buf = Vec::new();
    explain::render(&plan, &mut buf).expect("explain renders without write error");
    let out = String::from_utf8(buf).expect("explain output is utf-8");
    assert!(
        out.contains("system `system_pkg`"),
        "missing system header:\n{out}"
    );
    assert!(
        out.contains("Instances (1)"),
        "missing instances section:\n{out}"
    );
    assert!(
        out.contains("rate_hz = "),
        "missing resolved parameter:\n{out}"
    );
    assert!(out.contains("→"), "missing instance/binding arrow:\n{out}");
}

fn write_workspace_fixture(root: &Path) {
    fs::create_dir_all(root).expect("create workspace");
    fs::write(
        root.join("package.xml"),
        r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
    )
    .expect("write package.xml");
    fs::write(root.join("system.launch.xml"), "<launch />").expect("write launch");
    fs::write(
        root.join("record.json"),
        r#"{"node":[{"package":"demo_pkg","executable":"talker","name":"talker"}]}"#,
    )
    .expect("write record");
    fs::write(
        root.join("manifest.launch.yaml"),
        r#"version: 1
topics:
  /chatter:
    type: std_msgs/msg/String
    pub: [/talker]
    sub: [/talker]
"#,
    )
    .expect("write manifest");
    fs::write(
        root.join("talker.metadata.json"),
        r#"{
  "version": 1,
  "package": "demo_pkg",
  "component": "talker",
  "language": "rust",
  "executable": "talker",
  "exported_symbol": "nros_component_talker",
  "nodes": [{
    "id": "node_talker",
    "unresolved_name": {"value": "talker", "kind": "relative"},
    "namespace": null,
    "publishers": [{
      "id": "pub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null
    }],
    "subscribers": [{
      "id": "sub_chatter",
      "unresolved_topic": {"value": "chatter", "kind": "relative"},
      "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"},
      "qos": null,
      "callback": "cb_chatter"
    }],
    "timers": [],
    "services": [],
    "actions": []
  }],
  "callbacks": [{
    "id": "cb_chatter",
    "kind": "subscription",
    "group": null,
    "effects": [{"kind": "publishes", "entity": "pub_chatter"}],
    "source": {"artifact": "src/talker.rs", "line": 42, "column": 5}
  }],
  "parameters": [
    {"node": "node_talker", "name": "rate_hz", "default": 10, "read_only": false, "source": {"artifact": "src/talker.rs", "line": 10, "column": 1}}
  ],
  "trace": {"generator": "nros-metadata-rust", "package_manifest": "package.xml", "source_artifacts": ["src/talker.rs"]}
}"#,
    )
    .expect("write metadata");
}

/// Phase 126.B.7 acceptance — a package that declared itself a nros
/// component via `component_nros.toml` but never produced its
/// source-metadata JSON (most common cause: missing
/// `nros::component!` export) is the host-side surface for
/// `ComponentError::MissingExport`. The `metadata` command must
/// surface that case with the canonical diagnostic string instead of
/// silently producing an empty `metadata/` directory.
/// Verifies the orchestration metadata command flags a missing component export.
#[test]
fn orchestration_metadata_flags_missing_export() {
    let root = temp_workspace("metadata_missing_component_export");
    fs::create_dir_all(root.join("src/demo_pkg")).expect("create workspace");
    fs::write(
        root.join("package.xml"),
        r#"<package format="3"><name>system_pkg</name><version>0.1.0</version></package>"#,
    )
    .expect("write system package.xml");
    fs::write(
        root.join("src/demo_pkg/package.xml"),
        r#"<package format="3"><name>demo_pkg</name><version>0.1.0</version><depend>nros</depend></package>"#,
    )
    .expect("write component package.xml");
    // `component_nros.toml` declares the package as a nros component
    // and points at the source-metadata path the build is expected
    // to emit. We deliberately don't create that JSON file — the
    // metadata command should bail with MISSING_COMPONENT_EXPORT_ERROR.
    fs::write(
        root.join("src/demo_pkg/component_nros.toml"),
        r#"version = 1
package = "demo_pkg"
component = "talker"
language = "rust"

[linkage]
crate_name = "demo_pkg"
executable = "talker"
exported_symbol = "nros_component_talker"

[metadata]
source_metadata = "target/nros/metadata/talker.json"
generated_by = "cargo nros metadata"

[overrides]
default_namespace = "/demo"
parameters = {}
remaps = []
"#,
    )
    .expect("write component_nros.toml");

    let out_dir = root.join("build/system_pkg/nros");
    let err = metadata::run(metadata::Args {
        system_pkg: "system_pkg".to_string(),
        workspace: Some(root.clone()),
        out_dir: Some(out_dir),
        metadata: Vec::new(),
        build: false,
        nano_ros_workspace: None,
    })
    .expect_err("metadata command flags missing component export");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("package has no exported nros component"),
        "diagnostic should carry MISSING_COMPONENT_EXPORT_ERROR; got: {msg}"
    );
    assert!(
        msg.contains("demo_pkg"),
        "diagnostic should name the offending package; got: {msg}"
    );
    assert!(
        msg.contains("nros::component!"),
        "diagnostic should hint at the missing macro; got: {msg}"
    );
}

fn temp_workspace(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}
