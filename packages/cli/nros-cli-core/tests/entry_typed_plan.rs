//! Phase 240.2b (RFC-0043) — typed Entry plan seam, end-to-end in Rust.
//!
//! Drives `plan_from_launch` → `metadata::enrich_plan` → `emit_cpp::emit_typed`
//! against the `multi-node-workspace-cpp` template workspace + a synthetic
//! `nros-metadata.json` (the cmake-emitted shape). Proves the codegen reads the
//! launch topology, stamps each node's C++ class + header from the metadata, and
//! emits a TU that constructs + configures both components on the real executor —
//! without any cmake build (issue 0034: no compilation inside tests).

use nros_cli_core::codegen::entry::{self, metadata};

/// Repo-root-relative template workspace shipped in-tree.
fn template_ws() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = packages/cli/nros-cli-core
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .join("examples/templates/multi-node-workspace-cpp")
}

const METADATA: &str = r#"{
  "components": [
    {"name": "talker", "class": "talker_pkg::Talker",
     "class_header": "talker_pkg/Talker.hpp",
     "sources": ["src/Talker.cpp"], "deploy": ["native"],
     "pkg_dir": "/ws/src/talker_pkg", "lang": "cpp"},
    {"name": "listener", "class": "listener_pkg::Listener",
     "class_header": "listener_pkg/Listener.hpp",
     "sources": ["src/Listener.cpp"], "deploy": ["native"],
     "pkg_dir": "/ws/src/listener_pkg", "lang": "cpp"}
  ],
  "applications": [],
  "deploy_targets": {}
}"#;

#[test]
fn typed_plan_from_template_emits_constructed_components() {
    let ws = template_ws();
    if !ws
        .join("src/demo_bringup/launch/system.launch.xml")
        .is_file()
    {
        // Template missing (sparse checkout) — nothing to assert against.
        eprintln!("SKIP: template workspace absent at {}", ws.display());
        return;
    }

    let plan = entry::plan_from_launch(entry::PlanInput {
        workspace: &ws,
        launch_spec: "demo_bringup",
        board: Some("native".into()),
        arg_overrides: vec![],
    })
    .expect("plan from launch");

    // Two launch nodes in order: talker then listener.
    assert_eq!(plan.nodes.len(), 2);
    assert_eq!(plan.nodes[0].pkg, "talker_pkg");
    assert_eq!(plan.nodes[0].exec, "talker");

    let index = metadata::ComponentIndex::parse(METADATA).expect("metadata parse");
    let mut plan = plan;
    metadata::enrich_plan(&mut plan, &index).expect("enrich");

    assert_eq!(
        plan.nodes[0].class_name.as_deref(),
        Some("talker_pkg::Talker")
    );
    assert_eq!(
        plan.nodes[0].class_header.as_deref(),
        Some("talker_pkg/Talker.hpp")
    );

    let src = entry::emit_cpp::emit_typed(&plan).expect("emit typed");
    // Headers + construct + configure + real-executor entry, in launch order.
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
    assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
    assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
    assert!(src.contains("static ::listener_pkg::Listener __nros_comp_1;"));
    assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
    assert!(src.contains("::nros::board::NativeBoard::run_components(&__nros_entry_setup)"));
    // No legacy interpreter seam.
    assert!(!src.contains("__nros_component_"));
    assert!(!src.contains("NodeContext"));

    let pos_t = src.find("__nros_comp_0").unwrap();
    let pos_l = src.find("__nros_comp_1").unwrap();
    assert!(pos_t < pos_l, "launch order preserved");
}
