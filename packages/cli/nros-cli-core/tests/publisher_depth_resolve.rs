//! phase-454 W2 -- a publisher's `qos: { depth: N }` survives resolution.
//!
//! The unit tests in `entity_inventory.rs` feed `EntityInventory::from_model` a
//! hand-written model. This one produces the model the way a build does: the
//! pinned `nros-launch-resolve` over a launch file and its provider-sidecar
//! contract, exactly as `nros sync` would. Both halves have to work for the
//! declaration to reach the build, and only one of them was ever broken --
//! `pub_contract` in the resolver has carried publisher QoS all along, and
//! `from_model` dropped it on arrival.
//!
//! The fixture states a depth on one publisher and one subscriber and leaves
//! the other pair silent, because "nobody said" and "said a number" license
//! opposite actions and a fixture with only the first case cannot tell them
//! apart.
//!
//! Run with: `cargo test --manifest-path packages/cli/Cargo.toml --test publisher_depth_resolve`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::entity_inventory::{DeclaredDepths, EntityInventory, EntityKind};
use ros_launch_manifest_model::SystemModel;

/// The contract says `pub: { chatter: { qos: { depth: 8 } } }`; the publisher
/// row carries 8.
#[test]
fn a_contracted_publisher_depth_reaches_the_entity_inventory() {
    let m = resolve("depths");
    assert_eq!(
        m.contracts
            .pub_endpoints
            .get("/talker/chatter")
            .and_then(|c| c.qos.as_ref())
            .and_then(|q| q.depth),
        Some(8),
        "the resolver must carry publisher QoS into the model: {:?}",
        m.contracts.pub_endpoints
    );

    let inv = EntityInventory::from_model("fixture", &m).expect("the model describes wiring");
    let mut publishers: Vec<(Option<String>, Option<u32>)> = inv
        .components()
        .iter()
        .flat_map(|c| c.declaration.entities())
        .filter(|e| e.kind == EntityKind::Publisher)
        .map(|e| (e.name.clone(), e.depth))
        .collect();
    publishers.sort();
    assert_eq!(
        publishers,
        vec![
            (Some("/chatter".to_string()), Some(8)),
            // The silent publisher stays `None`. Never `0`: a depth of zero is
            // a QoS no endpoint can have, so it must not be how "not declared"
            // is spelled.
            (Some("/quiet".to_string()), None),
        ]
    );
}

/// ...and it lands in the depth table under its own kind, with the
/// publisher-scoped undeclared count beside it.
#[test]
fn the_resolved_publisher_depth_is_tabled_under_its_own_kind() {
    let inv = EntityInventory::from_model("fixture", &resolve("depths"))
        .expect("the model describes wiring");
    let DeclaredDepths::Resolved {
        rows,
        undeclared_publishers,
        undeclared_subscriptions,
        ..
    } = inv.declared_depths()
    else {
        panic!("a composed inventory resolves its depths");
    };
    assert_eq!(
        rows.iter()
            .map(|r| (r.kind.tag(), r.topic.as_str(), r.depth))
            .collect::<Vec<_>>(),
        vec![
            ("publisher", "/chatter", 8),
            ("subscription", "/chatter", 3),
        ]
    );
    assert_eq!(undeclared_publishers, 1, "/quiet's publisher said nothing");
    assert_eq!(undeclared_subscriptions, 1, "/quiet's subscriber did too");

    // And the sizing lists stay apart: the publisher's 8 must never reach the
    // list `nros-node/build.rs::subs_arena` counts against the SUBSCRIPTION
    // count, or every declaring image silently falls back to the worst case.
    let cmake = inv.to_cmake();
    assert!(
        cmake.contains("set(NROS_ENTITY_DECLARED_DEPTHS \"std_msgs/msg/Int32|/chatter=3\")\n"),
        "{cmake}"
    );
    assert!(
        cmake.contains(
            "set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER \"std_msgs/msg/Int32|/chatter=8\")\n"
        ),
        "{cmake}"
    );
}

/// Resolve `launch/<stem>.launch.xml` (with its `<stem>.contract.yaml`
/// sidecar) through the pinned resolver, by ABSOLUTE path (issue 0285).
fn resolve(stem: &str) -> SystemModel {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let resolver = repo.join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    assert!(
        resolver.is_file(),
        "nros-launch-resolve not built at {} -- run `just setup-launch-resolve`",
        resolver.display()
    );
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/publisher_depth");
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
        "publisher-depth-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
