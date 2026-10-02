//! issue 1564 -- the declared-QoS check has an ADOPTER, and it stays adopted.
//!
//! `NROS_ASSERT_DECLARED_*` fails the build when a C++ call site and its
//! contract state different QoS. Until issue 1564 nothing shipping exercised
//! it: the one `qos: depth:` in the tree was in a Rust leaf the check cannot
//! reach, and every C++ contract declared nothing. Worse, the configures that
//! could adopt it could not -- a multi-entry configure ABSTAINED for every
//! component, and the native road builds every entry of a workspace in one.
//!
//! `examples/workspaces/cpp` is the adopter: its `system.contract.yaml`
//! declares the listener's depth and `Listener.cpp` asserts its own QoS
//! against the rendered table. This test pins the two halves a reader cannot
//! see from either file alone:
//!
//! 1. the REAL contract, resolved by the pinned resolver exactly as `nros sync`
//!    resolves it, renders a `resolved` table with the listener's row -- through
//!    the SEVERAL-model path a native configure takes (the system model plus a
//!    sibling entry's model that never launches the listener);
//! 2. the component still asserts against it. A contract row nothing compares
//!    the code with is the state this issue was filed about.
//!
//! The negative half -- a mismatch in that image fails the build -- is a
//! compile, so it lives in the build stage, not here ("no compilation inside
//! tests"): `just check declared-qos-header` cases F-F4 hold the mechanism, and
//! issue 1564's resolution records the measured failure of the real image.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test declared_qos_adopter`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
};

use nros_cli_core::{
    cmd::entity_inventory::inventory_from_metadata_file,
    entity_inventory::{DeclaredQosHeaderTable, EntityInventory, render_declared_qos_header},
};
use ros_launch_manifest_model::SystemModel;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
/// files live in `$project/tmp/`, not the system temp dir).
fn scratch() -> PathBuf {
    let stamp = common::unique_stamp();
    let dir = repo().join("tmp").join(format!(
        "declared-qos-adopter-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Resolve one of the adopter's launch files the way `nros sync` does.
fn resolve(out: &Path, stem: &str) -> PathBuf {
    let bringup = repo().join("examples/workspaces/cpp/src/demo_bringup");
    let model = out.join(format!("{stem}_model.yaml"));
    let output = std::process::Command::new(common::pinned_launch_resolver())
        .arg(bringup.join(format!("launch/{stem}.launch.xml")))
        .arg("--bringup-root")
        .arg(&bringup)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for {stem}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    model
}

#[test]
fn the_cpp_workspace_listener_declares_a_depth_and_asserts_against_it() {
    let out = scratch();
    let system = resolve(&out, "system");
    let sibling = resolve(&out, "service_server");

    // The registered component population, as `nano_ros_node_register()` writes
    // it for this workspace: every component the native configure compiles.
    let metadata = out.join("nros-metadata.json");
    fs::write(
        &metadata,
        r#"{"components": [
  {"name": "talker", "pkg": "talker_pkg", "class": "talker_pkg::Talker"},
  {"name": "listener", "pkg": "listener_pkg", "class": "listener_pkg::Listener"},
  {"name": "add_server", "pkg": "service_server_pkg", "class": "service_server_pkg::AddServer"}
]}"#,
    )
    .expect("write metadata");

    // The several-model union a native configure renders for this component
    // (`nros ws entity-inventory --model A --model B --component ...`). Composed
    // here from the same library calls, because the verb's door also checks
    // the resolver pin `nros sync` stamps into a model, which a model resolved
    // straight from the pinned binary does not carry. The verb's own
    // several-model path is driven end to end by `just check
    // declared-qos-header` case H.
    let meta = inventory_from_metadata_file(&metadata).expect("metadata inventory");
    let tables: Vec<(String, DeclaredQosHeaderTable)> = [system, sibling]
        .iter()
        .map(|p| {
            let text = fs::read_to_string(p).expect("read model");
            let model = SystemModel::from_yaml_str(&text).expect("model parses");
            let wired = EntityInventory::from_model(p.display().to_string(), &model)
                .expect("both launch files carry a contract, so both describe wiring");
            (
                p.display().to_string(),
                meta.merged_per_kind_max(&wired).declared_qos_header_table(),
            )
        })
        .collect();
    let h = render_declared_qos_header("adopter", &DeclaredQosHeaderTable::union(&tables));
    let _ = fs::remove_dir_all(&out);

    assert!(
        h.contains("#define NROS_DECLARED_QOS_STATUS \"resolved\""),
        "the adopter's table must RESOLVE -- a refused one compiles every check off: {h}"
    );
    assert!(
        h.contains("#define NROS_DECLARED_QOS_ROW_COUNT 2"),
        "one declared subscription, two type spellings: {h}"
    );
    assert!(
        h.contains(
            "NROS_DECLARED_QOS_ROW(\"std_msgs::msg::dds_::Int32_\", \"/chatter\", 1, \
             NROS_DQ_UNDECLARED, NROS_DQ_UNDECLARED)"
        ),
        "examples/workspaces/cpp's system.contract.yaml declares `chatter: {{ qos: {{ depth: 1 \
         }} }}` for the listener: {h}"
    );

    // And the code still compares itself with that row.
    let listener = fs::read_to_string(
        repo().join("examples/workspaces/cpp/src/listener_pkg/src/Listener.cpp"),
    )
    .expect("read Listener.cpp");
    assert!(
        listener.contains("NROS_ASSERT_DECLARED_QOS(") && listener.contains("\"/chatter\""),
        "Listener.cpp no longer asserts its QoS against the declared table. The contract row \
         then states a number nothing checks, which is exactly what issue 1564 was filed about \
         -- keep the assertion, or move the adoption to another C++ image and update this test"
    );
}
