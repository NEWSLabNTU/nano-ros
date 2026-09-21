//! phase-446 F1 -- `params: {}` survives resolution, end to end.
//!
//! The unit tests in `entity_inventory.rs` feed `ParamDeclarations::from_model`
//! a hand-written model. This one produces the model the way a build does: the
//! pinned `nros-launch-resolve` over a launch file and its provider-sidecar
//! contract. The two fixtures differ in one line, `/b`'s `params: {}`, and
//! that line must decide between a sized store and a refusal. Before F1 the
//! resolver dropped the empty map, so both fixtures refused.
//!
//! Run with: `cargo test --manifest-path packages/cli/Cargo.toml --test param_declarations_resolve`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::entity_inventory::{EntityInventory, ParamDeclarations};
use ros_launch_manifest_model::SystemModel;

/// `/b` says `params: {}`: the image declares, and `/b` gets only its seeded
/// `use_sim_time` slot.
#[test]
fn an_empty_params_resolves_to_a_declared_store() {
    let m = resolve("declared");
    assert_eq!(
        m.contracts.node_params.get("/b").map(|p| p.len()),
        Some(0),
        "`params: {{}}` must reach the model as an empty entry: {:?}",
        m.contracts.node_params
    );
    let d = ParamDeclarations::from_model(&m);
    let ParamDeclarations::Declared { nodes, params } = &d else {
        panic!("every node declares, got {d:?}");
    };
    assert_eq!(nodes, &["/a", "/b"]);
    assert_eq!(
        params
            .iter()
            .map(|p| (p.node.as_str(), p.name.as_str()))
            .collect::<Vec<_>>(),
        [("/a", "rate")],
        "`/b` declares nothing"
    );
    let z = d.sizing().expect("a declared image is sized");
    assert_eq!(z.declared, 1);
    assert_eq!(
        z.max_parameters, 3,
        "/a: rate + use_sim_time, /b: use_sim_time alone"
    );
}

/// Without the `{}`, `/b` has said nothing, and the image refuses, naming it.
#[test]
fn a_missing_params_resolves_to_a_refusal_naming_the_node() {
    let m = resolve("silent");
    assert!(
        !m.contracts.node_params.contains_key("/b"),
        "no `params:` must give no entry: {:?}",
        m.contracts.node_params
    );
    let d = ParamDeclarations::from_model(&m);
    match &d {
        ParamDeclarations::Refused { reason } => {
            assert!(
                reason.contains("/b"),
                "the refusal names the node: {reason}"
            );
            assert!(!reason.contains("/a,"), "`/a` declared: {reason}");
        }
        other => panic!("a node with no `params:` must refuse, got {other:?}"),
    }
    assert_eq!(d.sizing(), None);
}

/// Issue 1436 -- the two inventories are INDEPENDENT, and a contract that
/// declares only `params:` must survive the ENTITY inventory's predicate.
///
/// The two tests above call `ParamDeclarations::from_model` directly, which is
/// exactly how this went unnoticed for two phases: BOTH roads compose the
/// parameter inventory beside `EntityInventory::from_model`, and the cargo road
/// nested the attach inside that call's `Some` arm. `declared` describes
/// parameters and no wiring, so the entity predicate answered `None`, the
/// nested attach never ran, and the resolve failed naming wiring the user was
/// never asked for.
///
/// Asserts the PREDICATE, not the road, because the predicate is the fix: a
/// parameter-only contract is an authored contract, and the inventory it yields
/// has zero entity rows -- the true answer for such an image, not the absence
/// of one.
#[test]
fn a_parameter_only_contract_is_still_an_authored_contract() {
    let m = resolve("declared");
    assert!(
        m.structure.topics.is_empty()
            && m.structure.services.is_empty()
            && m.structure.actions.is_empty(),
        "this fixture must stay parameter-only, or it stops covering issue 1436: {:?}",
        m.structure.topics
    );

    let mut inv = EntityInventory::from_model("param_declarations/declared", &m)
        .expect("a contract declaring only `params:` is authored, so the inventory composes");
    assert!(
        inv.is_empty(),
        "zero entity rows is the TRUE answer here, not a missing inventory"
    );

    // What each road then does, and the half that used to be dropped.
    inv.set_param_declarations(ParamDeclarations::from_model(&m));
    let ParamDeclarations::Declared { params, .. } = inv.param_declarations() else {
        panic!(
            "the parameter facts must survive composition, got {:?}",
            inv.param_declarations()
        );
    };
    assert_eq!(
        params
            .iter()
            .map(|p| (p.node.as_str(), p.name.as_str()))
            .collect::<Vec<_>>(),
        [("/a", "rate")],
    );
}

/// Resolve `launch/<stem>.launch.xml` (with its `<stem>.contract.yaml`
/// sidecar) through the pinned resolver, by ABSOLUTE path (issue 0285).
fn resolve(stem: &str) -> SystemModel {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let resolver = common::pinned_launch_resolver();
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/param_declarations");
    let out = temp_output(repo, stem);
    fs::create_dir_all(&out).expect("create model out dir");
    let model = out.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
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
    let text = fs::read_to_string(&model).expect("read the resolved model");
    let _ = fs::remove_dir_all(&out);
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
/// files live in `$project/tmp/`, not the system temp dir).
fn temp_output(repo: &Path, name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = repo.join("tmp").join(format!(
        "param-declarations-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
