//! Phase 240.2b (RFC-0043) — typed Entry plan seam, end-to-end in Rust.
//!
//! Drives `plan_from_model` → `metadata::enrich_plan` → `emit_cpp::emit_typed`
//! against the `multi-node-workspace-cpp` template's committed SystemModel + a
//! synthetic `nros-metadata.json` (the cmake-emitted shape). Proves the codegen
//! reads the model topology, stamps each node's C++ class + header from the
//! metadata, and emits a TU that constructs + configures both components on the
//! real executor — without any cmake build (issue 0034: no compilation inside
//! tests).
//!
//! Phase-296 R-code retired the launch-XML plan path this test used to drive;
//! the model is the only plan input now.

use nros_cli_core::codegen::entry::{self, metadata};

/// The template's committed model, shipped in-tree.
fn template_model() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = packages/cli/nros-cli-core
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .join(
            "examples/templates/multi-node-workspace-cpp/src/demo_bringup/config/system_model.yaml",
        )
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
    let model = template_model();
    assert!(
        model.is_file(),
        "template model missing at {} — it is committed in-tree, so its absence \
         is a repo defect, not a reason to skip",
        model.display()
    );

    let plan = entry::plan_from_model(&model, Some("native".into())).expect("plan from model");

    // Two nodes, in the model's own order: `structure.nodes` is a sorted map, so
    // `/listener` precedes `/talker`. The model has no launch order to preserve
    // (RFC-0050, early binding) — plan order is model order.
    assert_eq!(plan.nodes.len(), 2);
    assert_eq!(plan.nodes[0].pkg, "listener_pkg");
    assert_eq!(plan.nodes[0].exec, "listener");
    assert_eq!(plan.nodes[1].pkg, "talker_pkg");
    assert_eq!(plan.nodes[1].exec, "talker");

    let index = metadata::ComponentIndex::parse(METADATA).expect("metadata parse");
    let mut plan = plan;
    metadata::enrich_plan(&mut plan, &index).expect("enrich");

    assert_eq!(
        plan.nodes[0].class_name.as_deref(),
        Some("listener_pkg::Listener")
    );
    assert_eq!(
        plan.nodes[0].class_header.as_deref(),
        Some("listener_pkg/Listener.hpp")
    );

    let src = entry::emit_cpp::emit_typed(&plan).expect("emit typed");
    // Headers + construct + configure + real-executor entry, in plan order.
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
    assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
    assert!(src.contains("static ::listener_pkg::Listener __nros_comp_0;"));
    assert!(src.contains("static ::talker_pkg::Talker __nros_comp_1;"));
    assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
    // Phase 266: boot config blob always emitted; session name threaded from it.
    assert!(src.contains("NROS_BOOT_CONFIG_MAGIC"));
    assert!(src.contains("::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"));
    // No legacy interpreter seam.
    assert!(!src.contains("__nros_component_"));
    assert!(!src.contains("NodeContext"));

    let pos_l = src.find("__nros_comp_0").unwrap();
    let pos_t = src.find("__nros_comp_1").unwrap();
    assert!(pos_l < pos_t, "plan order preserved");
}
