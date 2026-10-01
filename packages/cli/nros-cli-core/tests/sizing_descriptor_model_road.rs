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
    sizing_descriptor::{
        DescriptorInputs, EntryLanguage, ModelHorizon, ModelImage, UNOBSERVED_ON_MODEL_ROAD_ISSUE,
        build, write_for_model,
    },
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
        bound_inventories: &[],
        horizon: ModelHorizon::new("a workspace cargo image"),
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

    // REFUSED — but only on the input that is MISSING, never for the road.
    //
    // issue 1393 closed the road-wide refusal: every one of these is composed
    // by the leaf road's own code from inputs the model road now carries (the
    // registered bound tables, the board's triple). THIS call hands over no
    // table, so the payload class refuses naming exactly that, and the
    // subscription's registration refuses because nothing observed it (issue
    // 1594) — two different reasons, each the row's own.
    //
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
    let payload = [
        ("wire_bound_bytes", bound.refusal()),
        ("storage_bytes", storage.refusal()),
        ("types.max_fields", fields.refusal()),
        ("types.max_kinds", kinds.refusal()),
        ("types.max_nested_depth", nested.refusal()),
    ];
    for (what, reason) in payload {
        let reason = reason.unwrap_or_else(|| panic!("`{what}` must be REFUSED with no table"));
        assert!(
            reason.contains("no message-bound table"),
            "`{what}`'s refusal must name the table nobody handed over: {reason}"
        );
        assert!(
            !reason.contains("issue 1393"),
            "`{what}` is not refused for the ROAD any more: {reason}"
        );
        // phase-457 W0.b — and it must name THIS road's input. The horizon gained
        // a second source (a standalone leaf's own declaration), and a refusal
        // that names the wrong one sends its reader after an artifact that does
        // not exist on their road. The leaf half is asserted in
        // `cmd::sizing_descriptor`'s `the_leaf_roads_refusal_names_the_declaration_not_a_model`.
        assert!(
            reason.contains("resolved SystemModel"),
            "`{what}`'s refusal on the MODEL road must name the model: {reason}"
        );
    }
    let reg = path
        .refusal()
        .expect("an unobserved subscription on an in-place backend refuses its path");
    assert!(reg.contains(UNOBSERVED_ON_MODEL_ROAD_ISSUE), "{reg}");
    assert!(reg.contains("resolved SystemModel"), "{reg}");

    // And the file says all of it: the refusals travel to the consumer, not to
    // a build log nobody kept (RFC-0100 D6).
    let body = fs::read_to_string(&written.path).expect("read the descriptor");
    assert_eq!(
        body.matches("issue 1393").count(),
        0,
        "no refusal on this road is about the road any more:\n{body}"
    );
    assert_eq!(
        body.matches(UNOBSERVED_ON_MODEL_ROAD_ISSUE).count(),
        // The two SUBSCRIPTIONS' `registration_path`, and nothing else: a
        // publisher has no receive slot to get wrong, so its composed answer
        // stands (phase-457 W3), and no other field is about the observation.
        2,
        "only an unobserved subscription's path names the observation gap:\n{body}"
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
        bound_inventories: &[],
        horizon: ModelHorizon::new("a workspace cargo image"),
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
        bound_inventories: &[],
        horizon: ModelHorizon::new("a cmake entry"),
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
        !why.contains(UNOBSERVED_ON_MODEL_ROAD_ISSUE) && !why.contains("no message-bound table"),
        "a declaration's own refusal must not be attributed to the road: {why}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// phase-457-payload W2 — THE ACCEPTANCE's model half: handed the bound tables
/// its interface closure links, the model road STATES what it refused.
///
/// The table is a real one, the committed `nros-std-msgs` bound inventory that
/// codegen emitted, not a fixture written for this test — the fixture's only
/// type is `std_msgs/msg/Int32`, and a hand-written row would test this
/// function against a number this function's author chose.
///
/// The PARITY half (both roads read one table the same way) is asserted where
/// the reader lives, in `leaf_payload_classes`; this one asserts that the
/// model road actually reaches it.
#[test]
fn bound_tables_turn_the_model_roads_payload_refusals_into_facts() {
    let dir = scratch("bounds");
    let model = resolve("policies", &dir);
    let inv = EntityInventory::from_model("fixture", &model).expect("the fixture describes wiring");
    let tables = [std_msgs_bound_table()];
    let written = write_for_model(&ModelImage {
        build_dir: &dir,
        entry: "policies",
        inventory: &inv,
        target_triple: Some("thumbv7em-none-eabihf".into()),
        host_build: false,
        heap_budget_bytes: Some(65_536),
        rmw: Some("zenoh".into()),
        bound_inventories: &tables,
        horizon: ModelHorizon::new("a cmake entry"),
    })
    .expect("the descriptor is written");
    let desc = &written.desc;
    let sub = desc
        .endpoints
        .iter()
        .find(|e| e.kind == EndpointKind::Subscription && e.topic == "/chatter")
        .expect("the contract declares a subscription on /chatter");

    // STATED now -- the two families issue 1393 named for this road.
    let bound = sub.wire_bound_bytes();
    assert!(
        bound.is_stated(),
        "handed the closure's bound table, `wire_bound_bytes` must be STATED on \
         the model road; got {:?}",
        bound.refusal()
    );
    for (what, fact) in [
        ("types.max_fields", desc.types.max_fields()),
        ("types.max_kinds", desc.types.max_kinds()),
        ("types.max_nested_depth", desc.types.max_nested_depth()),
    ] {
        assert!(
            fact.is_stated(),
            "`{what}` must be STATED once the schema shapes arrive with the \
             bounds; got {:?}",
            fact.refusal()
        );
    }

    // issue 1393 — `storage_bytes` is STATED too: it is the bound (above), the
    // board's pointer width (the triple) and the declared depth, all three of
    // which this road now has. phase-454 W14 refused it outright under the
    // horizon with a reason ("neither of its two sizes") that this very row
    // contradicted.
    assert!(
        sub.storage_bytes().is_stated(),
        "bound + triple + depth are all here, so the region must be STATED; got {:?}",
        sub.storage_bytes().refusal()
    );
    // UNCHANGED -- the bound table does not answer this one: an in-place
    // backend's row turns on an OBSERVED registration, which a model row does
    // not carry (issue 1594). A table that silently stated it would be claiming
    // a fact it does not hold.
    assert!(
        sub.registration_path().refusal().is_some(),
        "`registration_path` needs an observed registration, not a bound table"
    );

    // THE PARITY: where both roads answer, they answer ALIKE. The leaf road's
    // composer, handed the same inventory, the same table through the same
    // reader and the same board — what `write_for_leaf` builds, minus the
    // horizon and plus the one input only a leaf has (its language) — must state
    // every payload-class field the model road states, byte for byte. Field by
    // field rather than "the descriptors are equal", because the two DO differ
    // in exactly one place on purpose (the unobserved path's prose) and an
    // equality that excused it would excuse anything.
    let rows = nros_cli_core::leaf_payload_classes::bound_rows_from_tables(&tables)
        .expect("the committed table reads");
    let (bounds, schema_shapes) = nros_cli_core::leaf_payload_classes::project_bound_rows(rows);
    let leaf = build(&DescriptorInputs {
        entry: "policies".into(),
        inventory: Some(&inv),
        bounds,
        schema_shapes,
        bounds_error: None,
        target_triple: Some("thumbv7em-none-eabihf".into()),
        host_build: false,
        heap_budget_bytes: Some(65_536),
        params: Some(inv.param_declarations()),
        language: Some(EntryLanguage::CFamily),
        backend_schema: None,
        backend_dispatch: None,
        rmw: Some("zenoh".into()),
        horizon: None,
    });
    assert_eq!(leaf.endpoints.len(), desc.endpoints.len());
    for (l, m) in leaf.endpoints.iter().zip(&desc.endpoints) {
        assert_eq!((l.kind, &l.topic), (m.kind, &m.topic));
        assert_eq!(l.wire_bound_bytes(), m.wire_bound_bytes(), "{}", m.topic);
        assert_eq!(l.storage_bytes(), m.storage_bytes(), "{}", m.topic);
        assert_eq!(l.depth(), m.depth(), "{}", m.topic);
    }
    assert_eq!(leaf.types.max_fields(), desc.types.max_fields());
    assert_eq!(leaf.types.max_kinds(), desc.types.max_kinds());
    assert_eq!(leaf.types.max_nested_depth(), desc.types.max_nested_depth());
    assert_eq!(leaf.target.pointer_bytes(), desc.target.pointer_bytes());
    // And it is not vacuous: the subscription's region is a real number on
    // both sides, so the loop above compared two statements, not two refusals.
    assert!(
        leaf.endpoints
            .iter()
            .any(|e| e.kind == EndpointKind::Subscription && e.storage_bytes().is_stated())
    );
}

/// phase-457-payload W2 — a table the closure REGISTERED that is not on disk
/// yet is a per-field REFUSAL naming it. Not an error, and not the generic
/// "this road has no tables" reason.
///
/// This is the ordinary state of a clean tree on the non-Zephyr cmake lane:
/// the fragment is a BUILD-time output, so it is absent at the first configure
/// and present from the next. `NanoRosMessageBounds.cmake`'s aggregator reads
/// the same list with the same rule ("a promise rather than a fact"), and a
/// producer that failed the configure here would fail it for the most common
/// shape in the tree.
#[test]
fn a_registered_bound_table_not_yet_built_refuses_by_name() {
    let dir = scratch("pending");
    let model = resolve("policies", &dir);
    let inv = EntityInventory::from_model("fixture", &model).expect("the fixture describes wiring");
    let pending = dir.join("not_built_yet/nros_message_bounds.json");
    let tables = [pending.clone()];
    let written = write_for_model(&ModelImage {
        build_dir: &dir,
        entry: "policies",
        inventory: &inv,
        target_triple: Some("thumbv7em-none-eabihf".into()),
        host_build: false,
        heap_budget_bytes: Some(65_536),
        rmw: Some("zenoh".into()),
        bound_inventories: &tables,
        horizon: ModelHorizon::new("a cmake entry"),
    })
    .expect("a pending table must not fail the descriptor -- it refuses a field");
    let sub = written
        .desc
        .endpoints
        .iter()
        .find(|e| e.kind == EndpointKind::Subscription && e.topic == "/chatter")
        .expect("the contract declares a subscription on /chatter");
    let bound = sub.wire_bound_bytes();
    let reason = bound
        .refusal()
        .unwrap_or_else(|| panic!("a table not on disk cannot state a bound"));
    assert!(
        reason.contains("not_built_yet"),
        "the refusal must name the table that is missing, so its reader knows \
         which package's build has not run: {reason}"
    );
}

/// The committed `nros-std-msgs` bound inventory, by the path it is committed
/// at. Read, never written.
fn std_msgs_bound_table() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../interfaces/generated/humble/nros-std-msgs/nros_message_bounds.json");
    assert!(
        p.is_file(),
        "precondition: the committed std_msgs bound table is missing at {}",
        p.display()
    );
    p
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
