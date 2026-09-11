//! phase-454 W4 acceptance — the sizing descriptor is PORTABLE.
//!
//! > The descriptor is byte-identical across two checkouts of the same tree at
//! > different paths — issue 0320's portability rule. No absolute paths.
//!
//! This is not a style rule. A generated artifact that embeds its own checkout
//! path is different in every checkout, so every freshness comparison against it
//! is a false negative and every cache keyed on it misses. Issue 0320 measured
//! exactly that on the SystemModel — 67 models regenerated, plus a
//! `check-no-absolute-model-paths` gate — and this artifact is written by the
//! same command into the same `build/nros/` tree.
//!
//! The test builds ONE leaf twice, under two different roots, and compares the
//! rendered bytes. It reads real files from real paths on the way — the
//! inventory's `source` IS a path and the bound tables are found by walking one
//! — so a path reaching the output would have to survive the whole producer, and
//! that is the thing being measured.
//!
//! The tripwire inside `render` is a different check and covers a different
//! failure (an interpolated `Path::display()` in a refusal reason). Neither
//! subsumes the other: the tripwire fires on a path it recognises, and this
//! fires on ANY difference, including one nobody anticipated.

use std::path::Path;

use nros_cli_core::{
    leaf_entity_env::inventory_for_leaf,
    sizing_descriptor::{BackendSchema, DescriptorInputs, EntryLanguage, build},
};
use nros_sizing_descriptor::render;

/// A leaf with one talker component and one priced message package.
fn plant(root: &Path) {
    let leaf = root.join("talker");
    std::fs::create_dir_all(leaf.join("metadata")).unwrap();
    std::fs::create_dir_all(leaf.join("generated/std_msgs")).unwrap();

    std::fs::write(
        leaf.join("Cargo.toml"),
        "[package]\nname = \"talker\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(
        leaf.join("system.toml"),
        "[image.native]\nboard = \"native\"\n",
    )
    .unwrap();
    // What `nros sync`'s metadata probe writes.
    std::fs::write(
        leaf.join("metadata/talker.json"),
        r#"{
          "version": 1, "package": "talker", "component": "talker",
          "nodes": [{ "id": "talker",
            "publishers": [],
            "subscribers": [{"id": "/chatter",
              "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"}}],
            "timers": [{"id": "on_tick"}],
            "services": [], "actions": [] }]
        }"#,
    )
    .unwrap();
    // What codegen writes beside the generated crate.
    std::fs::write(
        leaf.join("generated/std_msgs/nros_message_bounds.json"),
        r#"{
          "schema_version": 1,
          "producer": "nros-codegen",
          "package": "std_msgs",
          "derivation": "nros_serdes::size::max_serialized_size",
          "types": [
            { "type_name": "std_msgs/msg/String", "state": "bounded",
              "tx_max_serialized_size": 1166, "rx_max_serialized_size": 1170 }
          ]
        }"#,
    )
    .unwrap();
}

/// Render the descriptor for the leaf planted under `root`.
fn descriptor_text(root: &Path) -> String {
    let leaf = root.join("talker");
    let (inv, _unprobeable) = inventory_for_leaf(&leaf).expect("the probe output parses");
    assert!(
        !inv.is_empty(),
        "precondition: the planted metadata produced an inventory -- without one this test \
         would compare two empty descriptors and pass while proving nothing"
    );
    // The inventory's `source` IS a path, and it differs between the two roots.
    // Asserted rather than assumed: if it ever stopped being a path, this test
    // would keep passing while no longer exercising the thing it exists for.
    assert!(
        inv.source.contains(root.to_str().unwrap()),
        "precondition: the inventory records its source path ({}), so a leak has something to \
         leak",
        inv.source
    );
    let bounds = nros_cli_core::leaf_payload_classes::leaf_bound_inventory(&leaf)
        .expect("the planted bound inventory parses");
    assert!(
        bounds.iter().any(|(n, _)| n == "std_msgs/msg/String"),
        "precondition: the bound table was found by walking a path under {}",
        root.display()
    );

    let inputs = DescriptorInputs {
        entry: "native".into(),
        inventory: Some(&inv),
        bounds,
        bounds_error: None,
        target_triple: Some("thumbv7em-none-eabihf".into()),
        host_build: false,
        heap_budget_bytes: Some(65536),
        language: Some(EntryLanguage::Rust),
        backend_schema: Some(BackendSchema::Schemaless),
        rmw: Some("zenoh".into()),
    };
    render(&build(&inputs))
}

#[test]
fn the_descriptor_is_byte_identical_across_two_checkouts_at_different_paths() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::Builder::new()
        // A deliberately different length as well as a different name: a path
        // that leaked as a fixed-width field would still differ, but one that
        // leaked only as a basename of equal length might not.
        .prefix("a-much-longer-checkout-name-")
        .tempdir()
        .unwrap();
    assert_ne!(a.path(), b.path());

    plant(a.path());
    plant(b.path());

    let first = descriptor_text(a.path());
    let second = descriptor_text(b.path());

    assert_eq!(
        first,
        second,
        "the sizing descriptor differs between two checkouts of one tree.\n\
         Issue 0320: a generated artifact that embeds its checkout path makes every \
         freshness comparison against it a false negative.\n--- {} ---\n{first}\n--- {} ---\n{second}",
        a.path().display(),
        b.path().display(),
    );
    // And it is not vacuously equal because it is empty.
    assert!(first.contains("/chatter"), "{first}");
    assert!(first.contains("wire_bound_bytes = 1170"), "{first}");
}
