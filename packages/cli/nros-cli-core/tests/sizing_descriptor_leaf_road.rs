//! phase-457 W0.b (issues 1407 / 1378) — the THIRD descriptor producer, on the
//! road that had none and the road that FAILED.
//!
//! The unit tests in `cmd/sizing_descriptor.rs` feed this producer synthetic
//! leaves. This one runs it over the two REAL leaves issue 1378 was filed
//! against — `examples/qemu-armv7a-nuttx/{c,cpp}/action-server`, measured
//! 2026-09-20 exhausting `ZPICO_MAX_QUERYABLES` at boot — and holds the
//! descriptor to the numbers that boot needs.
//!
//! **What this asserts, and why it is the acceptance rather than "a file was
//! written".** `NROS_DECLARED_TL_PUBLISHERS` was the carrier that fixed 1378,
//! and until issue 1649 this test required the descriptor to AGREE with it,
//! because a second derivation of one number agrees until the day it does not
//! (issue 1025). Issue 1649 measured the two interchangeable on every road and
//! deleted the carrier, so there is one derivation left: the descriptor must
//! state the one cache queryable an action server owes for `/status` and the
//! three service queryables it serves, and the carrier verb must no longer
//! state either.
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

/// What `examples/qemu-armv7a-nuttx/<lang>/action-server` creates, stated as
/// a list. Issue 1556: the leaf itself states `entities = "census"` now -- its
/// entities are its program's, read from a census a BUILD takes -- and a test
/// takes no census (no compilation inside tests). So the leaf is copied and the
/// copy states the list its census records (measured equal, issue 1556): the
/// subject here is the descriptor road agreeing with the carrier for one input,
/// and the opt-in itself is covered by `the_census_opt_in_refuses_without_a_census`.
const ACTION_SERVER: &str = "action_server:example_interfaces/action/Fibonacci:/fibonacci";

/// A copy of the REAL leaf's `system.toml` + `CMakeLists.txt` under `into`,
/// with its `entities = "census"` replaced by `entities` (TOML).
fn leaf_copy(lang: &str, into: &Path, entities: &str) -> PathBuf {
    let real = repo().join(format!("examples/qemu-armv7a-nuttx/{lang}/action-server"));
    assert!(
        real.join("system.toml").is_file(),
        "{}: the leaf issue 1378 was filed against must exist -- if it moved, \
         re-point this test rather than deleting it",
        real.display()
    );
    let system = std::fs::read_to_string(real.join("system.toml")).expect("reads");
    assert!(
        system.contains("entities = \"census\""),
        "{}: expected the census opt-in (issue 1556); update this test with the leaf",
        real.display()
    );
    let leaf = into.join(format!("{lang}-action-server"));
    std::fs::create_dir_all(&leaf).expect("mkdir");
    std::fs::write(
        leaf.join("system.toml"),
        system.replace("entities = \"census\"", &format!("entities = {entities}")),
    )
    .expect("write");
    std::fs::copy(real.join("CMakeLists.txt"), leaf.join("CMakeLists.txt")).expect("copy");
    leaf
}

/// Every field default, so a case states only what it is about.
fn args() -> SizingDescriptorArgs {
    SizingDescriptorArgs {
        descriptor: None,
        output_cmake: None,
        from_model: Vec::new(),
        composed_entry: Vec::new(),
        from_leaf: None,
        // phase-457-payload W2 -- no tables: the payload class refuses,
        // naming the missing registration (issue 1393 closed the road).
        bound_inventory: Vec::new(),
        build_dir: None,
        entry: None,
        metadata: None,
        workspace: None,
        rmw: None,
        target_triple: None,
        host_build: false,
        heap_budget_bytes: None,
        road: None,
    }
}

/// THE ACCEPTANCE: the two leaves issue 1378 measured failing get a descriptor,
/// and it states the counts their boot needs.
///
/// Both are `[[component]] entities` declarations with no bringup and no
/// SystemModel, so `write_for_model` cannot reach either — which is exactly the
/// mechanism issue 1407 records as keeping the carrier alive.
#[test]
fn the_leaves_issue_1378_measured_get_a_descriptor_that_states_their_queryables() {
    let dir = tempfile::tempdir().expect("scratch");
    let mut checked = 0;
    for lang in ["c", "cpp"] {
        let leaf = leaf_copy(lang, dir.path(), &format!("[\"{ACTION_SERVER}\"]"));

        // The verb the cmake leaf road calls states the infrastructure and
        // node facts, and no longer the application's counts (issue 1649).
        let facts = entity_facts::facts_from_leaf(&leaf)
            .expect("the leaf reads")
            .expect("it declares entities, so the verb does not abstain");
        for retired in [
            "NROS_DECLARED_TL_PUBLISHERS",
            "NROS_DECLARED_SERVICE_SERVERS",
        ] {
            assert!(
                !facts.contains_key(retired),
                "{lang}: `{retired}` was retired onto the descriptor (issue 1649): {facts:?}"
            );
        }

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
        // The fact 1378 is about: an action server owes a cache queryable for
        // the `/status` publisher no contract mentions.
        assert_eq!(
            from_desc.stated(),
            Some(&1),
            "{lang}: one action server, one TRANSIENT_LOCAL `/status` publisher"
        );
        // And the other half of the application's queryables: an action server
        // is three services on the wire.
        assert_eq!(
            desc.image.service_server_queryables().stated(),
            Some(&3),
            "{lang}: one action server, three service queryables"
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
    let leaf = leaf_copy("c", dir.path(), &format!("[\"{ACTION_SERVER}\"]"));
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

/// Issue 1556 -- the leaves as they ARE: `entities = "census"` with no census
/// taken refuses on BOTH readers (the carrier verb and this descriptor road),
/// naming the command that takes it. Never a silent fall back to the RMW's
/// guess, which for this leaf is issue 1142's tens of kilobytes.
#[test]
fn the_census_opt_in_refuses_without_a_census() {
    let dir = tempfile::tempdir().expect("scratch");
    let leaf = leaf_copy("cpp", dir.path(), "\"census\"");
    let err = entity_facts::facts_from_leaf(&leaf)
        .expect_err("no census, no answer")
        .to_string();
    assert!(err.contains("entity-census take --leaf"), "{err}");
    let err = run(SizingDescriptorArgs {
        from_leaf: Some(leaf.clone()),
        build_dir: Some(dir.path().join("b")),
        entry: Some("e".into()),
        host_build: true,
        ..args()
    })
    .expect_err("the descriptor road refuses too")
    .to_string();
    assert!(err.contains("entity-census take --leaf"), "{err}");
}
