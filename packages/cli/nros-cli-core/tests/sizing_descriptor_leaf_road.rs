//! phase-457 W0.b (issues 1407 / 1378) — the THIRD descriptor producer, on the
//! road that had none and the road that FAILED.
//!
//! The unit tests in `cmd/sizing_descriptor.rs` feed this producer synthetic
//! leaves. This one runs it over the two REAL leaves issue 1378 was filed
//! against — `examples/qemu-armv7a-nuttx/{c,cpp}/action-server`, measured
//! 2026-09-20 exhausting `ZPICO_MAX_QUERYABLES` at boot — and holds the new road
//! to the carrier that already works there.
//!
//! **What this asserts, and why it is the acceptance rather than "a file was
//! written".** `NROS_DECLARED_TL_PUBLISHERS` is the carrier that fixed 1378; the
//! descriptor is what is meant to replace it. Those two are only
//! interchangeable if they AGREE, and the way this campaign's defects have
//! repeatedly survived is a second derivation of one number that agrees until
//! the day it does not (issue 1025). So the test computes the fact BOTH ways —
//! through `cmd::entity_facts::facts_from_leaf` (the carrier) and through
//! `nros_sizing_descriptor::transient_local_publishers` over the descriptor this
//! road now writes — and requires the same answer. One rule, two roads.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test sizing_descriptor_leaf_road`

use std::path::{Path, PathBuf};

use nros_cli_core::cmd::{
    entity_facts,
    sizing_descriptor::{SizingDescriptorArgs, run},
};

/// The nano-ros repo root (`packages/cli/nros-cli-core` → up three).
fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("nano-ros workspace ancestor")
        .to_path_buf()
}

/// Every field default, so a case states only what it is about.
fn args() -> SizingDescriptorArgs {
    SizingDescriptorArgs {
        descriptor: None,
        output_cmake: None,
        from_model: None,
        from_leaf: None,
        // phase-457-payload W2 -- no tables: the refusal naming 1393.
        bound_inventory: Vec::new(),
        build_dir: None,
        entry: None,
        metadata: None,
        rmw: None,
        target_triple: None,
        host_build: false,
        heap_budget_bytes: None,
        road: None,
    }
}

/// THE ACCEPTANCE: the two leaves issue 1378 measured failing get a descriptor,
/// and it states the cache-queryable count their carrier states.
///
/// Both are `[[component]] entities` declarations with no bringup and no
/// SystemModel, so `write_for_model` cannot reach either — which is exactly the
/// mechanism issue 1407 records as keeping the carrier alive.
#[test]
fn the_leaves_issue_1378_measured_get_a_descriptor_that_agrees_with_the_carrier() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut checked = 0;
    for lang in ["c", "cpp"] {
        let leaf = repo().join(format!("examples/qemu-armv7a-nuttx/{lang}/action-server"));
        assert!(
            leaf.join("system.toml").is_file(),
            "{}: the leaf issue 1378 was filed against must exist — if it moved, \
             re-point this test rather than deleting it",
            leaf.display()
        );

        // The CARRIER's answer, from the verb the cmake leaf road already calls.
        let facts = entity_facts::facts_from_leaf(&leaf)
            .expect("the leaf reads")
            .expect("it declares entities, so the verb does not abstain");
        let carrier = facts
            .get(entity_facts::TL_PUBLISHERS)
            .expect("the carrier states the cache-queryable count for this leaf");

        // The DESCRIPTOR's answer, from the road this wave adds.
        let build_dir = dir.path().join(lang);
        run(SizingDescriptorArgs {
            from_leaf: Some(leaf.clone()),
            build_dir: Some(build_dir.clone()),
            entry: Some(format!("{lang}_action_server")),
            host_build: true,
            ..args()
        })
        .expect("a leaf that declares its entities gets a descriptor");
        let path =
            nros_sizing_descriptor::descriptor_path(&build_dir, &format!("{lang}_action_server"));
        assert!(
            path.is_file(),
            "{}: this road wrote no descriptor for a leaf that declares an action server",
            path.display()
        );
        let desc = nros_sizing_descriptor::read(&path).expect("it reads back through the schema");

        let from_desc = nros_sizing_descriptor::transient_local_publishers(&desc);
        assert_eq!(
            from_desc.stated().map(|n| n.to_string()).as_deref(),
            Some(carrier.as_str()),
            "{lang}: the descriptor and the carrier must derive ONE number — \
             descriptor {from_desc:?}, carrier {carrier}"
        );
        // And it is the fact 1378 is about: an action server owes a cache
        // queryable for the `/status` publisher no contract mentions.
        assert_eq!(
            from_desc.stated(),
            Some(&1),
            "{lang}: one action server, one TRANSIENT_LOCAL `/status` publisher"
        );

        // The endpoint table is the declaration, not a summary of it.
        assert_eq!(
            desc.endpoints.len(),
            1,
            "{lang}: {:?}",
            desc.endpoints.iter().map(|e| &e.topic).collect::<Vec<_>>()
        );
        assert_eq!(
            desc.endpoints[0].kind,
            nros_sizing_descriptor::EndpointKind::ActionServer
        );
        // `NROS_DECLARED_NODES` and `[image] node_count` are the same fact.
        assert_eq!(
            desc.image.node_count().stated().map(|n| n.to_string()),
            facts.get("NROS_DECLARED_NODES").cloned(),
            "{lang}: the node table count must agree across the two roads"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "both language leaves must be measured");
}

/// The descriptor is RELATIVE and self-contained on this road too (issue 0320).
///
/// `write_for_model` runs `portability_violation` against the directories it read
/// from, and a leaf road reads from a path under the checkout — so the same rule
/// that makes two checkouts render identical bytes has to hold here, or every
/// freshness comparison against this file is a lie. Asserted by reading the bytes
/// rather than by trusting the producer's own guard.
#[test]
fn the_leaf_roads_descriptor_carries_no_absolute_path() {
    let dir = tempfile::tempdir().expect("scratch");
    let leaf = repo().join("examples/qemu-armv7a-nuttx/c/action-server");
    let build_dir = dir.path().join("b");
    run(SizingDescriptorArgs {
        from_leaf: Some(leaf.clone()),
        build_dir: Some(build_dir.clone()),
        entry: Some("e".into()),
        host_build: true,
        ..args()
    })
    .expect("written");
    let body = std::fs::read_to_string(nros_sizing_descriptor::descriptor_path(&build_dir, "e"))
        .expect("reads");
    for needle in [
        repo().display().to_string(),
        leaf.display().to_string(),
        dir.path().display().to_string(),
    ] {
        assert!(
            !body.contains(&needle),
            "the descriptor leaked `{needle}`:\n{body}"
        );
    }
}
