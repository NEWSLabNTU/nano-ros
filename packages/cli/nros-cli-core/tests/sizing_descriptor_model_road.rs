//! phase-454 W14 (RFC-0100 D4/D6) — the SECOND descriptor producer, end to end.
//!
//! The unit tests in `sizing_descriptor.rs` feed the producer a hand-built
//! `EntityInventory`. This one produces the model the way a build does — the
//! pinned `nros-launch-resolve` over a launch file and its contract sidecar —
//! and then writes a descriptor from it the way `nros build` and
//! `nano_ros_entry()` do. Both halves have to work for a declaration to reach a
//! consumer, and W12's own measurement is what happens when only one does: a
//! descriptor that reached every consumer correctly and stated nothing any of
//! them could size from.
//!
//! THE RULING this asserts (owner's, RFC-0100 D6):
//!
//! > Emit what the SystemModel knows. REFUSE every field you cannot source. Do
//! > not invent a number, and do not fall back to one.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test sizing_descriptor_model_road`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::{
    entity_inventory::EntityInventory,
    sizing_descriptor::{MODEL_ONLY_ISSUE, ModelImage, write_for_model},
};
use nros_sizing_descriptor::{Basis, EndpointKind, History, Reliability, Status};
use ros_launch_manifest_model::SystemModel;

/// THE ACCEPTANCE: a contract reaches a model-only descriptor, stated; and
/// every field that needs a leaf refuses, naming the tracked follow-up.
#[test]
fn a_contract_reaches_the_model_road_and_the_leaf_facts_refuse_by_name() {
    let dir = scratch("acceptance");
    let model = resolve("policies", &dir);
    let inv = EntityInventory::from_model("fixture", &model).expect("the fixture describes wiring");
    let written = write_for_model(&ModelImage {
        build_dir: &dir,
        entry: "policies",
        inventory: &inv,
        target_triple: Some("thumbv7em-none-eabihf".into()),
        host_build: false,
        heap_budget_bytes: Some(65_536),
        rmw: Some("zenoh".into()),
        road: "a workspace cargo image",
    })
    .expect("the descriptor is written");
    let desc = &written.desc;

    // The file is where the ONE path rule says, so a consumer holding only the
    // build directory finds it without knowing this road.
    assert_eq!(
        written.path,
        nros_sizing_descriptor::descriptor_path(&dir, "policies")
    );
    assert!(written.path.is_file());

    // STATED — the declaration, in full.
    assert_eq!(desc.meta.basis, Basis::Contract);
    assert_eq!(desc.meta.status, Status::Partial);
    let sub = desc
        .endpoints
        .iter()
        .find(|e| e.kind == EndpointKind::Subscription && e.topic == "/chatter")
        .expect("the contract declares a subscription on /chatter");
    assert_eq!(sub.depth().stated(), Some(&3));
    assert_eq!(sub.history().stated(), Some(&History::KeepLast));
    assert_eq!(sub.reliability().stated(), Some(&Reliability::BestEffort));
    assert!(sub.durability().is_stated());
    assert_eq!(desc.image.node_count().stated(), Some(&2));
    assert_eq!(desc.image.backend_count().stated(), Some(&1));
    assert_eq!(desc.image.subscriber_count().stated(), Some(&2));
    // `[target]` is the BOARD's, not this process's (RFC-0100 D1) — 4 on the
    // thumbv7em triple above, where a host build script would have said 8.
    assert_eq!(desc.target.pointer_bytes().stated(), Some(&4));

    // REFUSED — the five fields that need a leaf's inventories, each naming
    // the issue that tracks closing the gap.
    // Each `Fact` bound to a name first: the accessors return owned values, so
    // a `.refusal()` in an array literal borrows a temporary that dies at the
    // semicolon.
    let (bound, storage, path) = (
        sub.wire_bound_bytes(),
        sub.storage_bytes(),
        sub.registration_path(),
    );
    let (fields, kinds, nested) = (
        desc.types.max_fields(),
        desc.types.max_kinds(),
        desc.types.max_nested_depth(),
    );
    let refusals = [
        ("wire_bound_bytes", bound.refusal()),
        ("storage_bytes", storage.refusal()),
        ("registration_path", path.refusal()),
        ("types.max_fields", fields.refusal()),
        ("types.max_kinds", kinds.refusal()),
        ("types.max_nested_depth", nested.refusal()),
    ];
    for (what, reason) in refusals {
        let reason = reason.unwrap_or_else(|| panic!("`{what}` must be REFUSED on the model road"));
        assert!(
            reason.contains(MODEL_ONLY_ISSUE),
            "`{what}`'s refusal must name {MODEL_ONLY_ISSUE}: {reason}"
        );
    }

    // And the file says all of it: the refusals travel to the consumer, not to
    // a build log nobody kept (RFC-0100 D6).
    let body = fs::read_to_string(&written.path).expect("read the descriptor");
    assert_eq!(
        body.matches(MODEL_ONLY_ISSUE).count(),
        // Per row: registration_path + wire_bound_bytes on all four, plus
        // storage_bytes on the two subscriptions; plus the three `[types]`.
        4 * 2 + 2 + 3,
        "every refusal this road writes names the follow-up:\n{body}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Issue 0320 — the descriptor carries NO absolute path, on this road too.
///
/// Two checkouts of one tree at different paths must render identical bytes, or
/// every freshness comparison against the file is a lie. The model road has a
/// new way to break it that the leaf road did not: the inventory's `source` is
/// a resolved MODEL PATH, and it is what a careless refusal reason would
/// interpolate.
#[test]
fn a_model_road_descriptor_carries_no_path_from_this_checkout() {
    let dir = scratch("portable");
    let model = resolve("policies", &dir);
    let inv =
        EntityInventory::from_model(dir.join("system_model.yaml").display().to_string(), &model)
            .expect("the fixture describes wiring");
    let written = write_for_model(&ModelImage {
        build_dir: &dir,
        entry: "policies",
        inventory: &inv,
        target_triple: None,
        host_build: true,
        heap_budget_bytes: None,
        rmw: Some("zenoh".into()),
        road: "a workspace cargo image",
    })
    .expect("the descriptor is written");
    let body = fs::read_to_string(&written.path).expect("read the descriptor");
    assert!(
        !body.contains(&dir.display().to_string()),
        "the build dir leaked into the descriptor:\n{body}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `keep_all` still refuses `depth` on this road, with the DECLARATION's own
/// reason rather than the road's.
///
/// The one RFC-0100 D6 trigger that ships a too-small buffer rather than a
/// too-large one, and the horizon must not swallow it: a reader told "no bound
/// inventory" here would go looking for codegen.
#[test]
fn keep_all_still_refuses_its_depth_on_the_model_road() {
    let dir = scratch("keep_all");
    let model = resolve("keep_all", &dir);
    let inv = EntityInventory::from_model("fixture", &model).expect("the fixture describes wiring");
    let written = write_for_model(&ModelImage {
        build_dir: &dir,
        entry: "keep_all",
        inventory: &inv,
        target_triple: None,
        host_build: true,
        heap_budget_bytes: None,
        rmw: Some("zenoh".into()),
        road: "a cmake entry",
    })
    .expect("the descriptor is written");
    let sub = written
        .desc
        .endpoints
        .iter()
        .find(|e| e.kind == EndpointKind::Subscription)
        .expect("the fixture declares a subscription");
    let depth = sub.depth();
    let why = depth.refusal().expect("keep_all refuses depth");
    assert!(why.contains("KEEP_ALL"), "{why}");
    assert!(
        !why.contains(MODEL_ONLY_ISSUE),
        "a declaration's own refusal must not be attributed to the road: {why}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Resolve one of the `qos_policies` fixtures into `out`, the way a build does.
fn resolve(stem: &str, out: &Path) -> SystemModel {
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
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qos_policies");
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
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp files
/// live in `$project/tmp/`, not the system temp dir).
fn scratch(name: &str) -> PathBuf {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = repo.join("tmp").join(format!(
        "sizing-model-road-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch dir");
    dir
}
