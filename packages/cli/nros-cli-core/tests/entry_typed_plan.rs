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
/// Resolve the template workspace's bringup into a temp dir and return the
/// model's path.
///
/// This used to open the committed
/// `…/multi-node-workspace-cpp/src/demo_bringup/config/system_model.yaml` and
/// assert that its absence "is a repo defect". phase-330 W4 inverted that: the
/// SystemModel is a build artifact, committing one is now the defect
/// (`check-no-tracked-models.sh`), and the file is gone — so the assertion
/// failed on a condition the repo deliberately created (issue 0414).
///
/// A test that needs a model produces one the way the build does.
fn template_model(dir: &std::path::Path) -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = packages/cli/nros-cli-core
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let bringup = repo.join("examples/templates/multi-node-workspace-cpp/src/demo_bringup");
    let resolver = repo.join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    if !resolver.is_file() {
        eprintln!(
            "[SKIPPED] nros-launch-resolve not built at {} — run `just setup-launch-resolve`",
            resolver.display()
        );
        return std::path::PathBuf::new();
    }
    let out = dir.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
        .arg(bringup.join("launch/system.launch.xml"))
        .arg("--bringup-root")
        .arg(&bringup)
        .arg("--system")
        .arg(bringup.join("system.toml"))
        .arg("-o")
        .arg(&out)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for the template bringup:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    out
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
    let tmp = tempfile::tempdir().expect("tempdir");
    let model = template_model(tmp.path());
    if model.as_os_str().is_empty() {
        return; // resolver not built — the helper printed the skip reason
    }

    let plan = entry::plan_from_model(&model, Some("native".into())).expect("plan from model");

    // Two nodes, in LAUNCH order — the template's `system.launch.xml` lists
    // talker then listener. This asserted the reverse until issue 0382:
    // `structure.nodes` used to be a sorted map, so the plan came out
    // alphabetically and the comment here reasoned that the model "has no launch
    // order to preserve". The resolver now carries an IndexMap precisely so it
    // does, which makes construct order match what the author wrote.
    assert_eq!(plan.nodes.len(), 2);
    assert_eq!(plan.nodes[0].pkg, "talker_pkg");
    assert_eq!(plan.nodes[0].exec, "talker");
    assert_eq!(plan.nodes[1].pkg, "listener_pkg");
    assert_eq!(plan.nodes[1].exec, "listener");

    let index = metadata::ComponentIndex::parse(METADATA).expect("metadata parse");
    let mut plan = plan;
    metadata::enrich_plan(&mut plan, &index).expect("enrich");

    // nodes[0] is the talker now (launch order, issue 0382) — enrichment must
    // follow the plan's order, not the metadata's.
    assert_eq!(
        plan.nodes[0].class_name.as_deref(),
        Some("talker_pkg::Talker")
    );
    assert_eq!(
        plan.nodes[0].class_header.as_deref(),
        Some("talker_pkg/Talker.hpp")
    );
    assert_eq!(
        plan.nodes[1].class_name.as_deref(),
        Some("listener_pkg::Listener")
    );

    let src = entry::emit_cpp::emit_typed(&plan).expect("emit typed");
    // Headers + construct + configure + real-executor entry, in plan order.
    assert!(src.contains("#include \"talker_pkg/Talker.hpp\""));
    assert!(src.contains("#include \"listener_pkg/Listener.hpp\""));
    // comp_0 is the talker: emit order follows PLAN order, which follows LAUNCH
    // order since issue 0382 (it was alphabetical while nodes were a sorted map).
    assert!(src.contains("static ::talker_pkg::Talker __nros_comp_0;"));
    assert!(src.contains("static ::listener_pkg::Listener __nros_comp_1;"));
    assert!(src.contains("__nros_comp_0.configure(__nros_node_0)"));
    // Phase 266: boot config blob always emitted; session name threaded from it.
    assert!(src.contains("NROS_BOOT_CONFIG_MAGIC"));
    assert!(src.contains("::nros::board::NativeBoard::run_components(nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup)"));
    // No legacy interpreter seam.
    assert!(!src.contains("__nros_component_"));
    assert!(!src.contains("NodeContext"));

    let pos_first = src.find("__nros_comp_0").unwrap();
    let pos_second = src.find("__nros_comp_1").unwrap();
    assert!(pos_first < pos_second, "plan order preserved");
}
