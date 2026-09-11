//! phase-454 W8 (RFC-0100 D9) — what `buffer:` and the two rates SURVIVE.
//!
//! The unit tests in `entity_inventory.rs` and `queue_depth.rs` prove the
//! ladder, the arithmetic and both diagnostics over rows built by hand. This
//! one asks the question those cannot: does a contract stating all three facts
//! deliver all three to the build? It produces the model the way a build does —
//! the pinned `nros-launch-resolve` over a launch file and its contract
//! sidecar, exactly as `nros sync` would.
//!
//! The answer is NO, and it is the wave's central finding rather than a
//! footnote, so it is measured here rather than asserted in prose:
//!
//! | fact | contract key | in the SystemModel? |
//! | --- | --- | --- |
//! | publish rate | `topics.<t>.rate_hz` | yes |
//! | publish rate | `<node>.pub.<ep>.min_rate_hz` | yes |
//! | drain rate | `<node>.paths.<p>.trigger.timer.rate_hz` | **no** |
//! | discipline | `<node>.sub.<ep>.buffer` | **no** |
//!
//! Both missing halves are dropped by the MODEL SCHEMA, not by nano-ros. The
//! resolver PARSES them, VALIDATES them (`buffer` outside `state: true` is a
//! parse-time error) and REASONS about them — it emits a `[queue-drain-rate]`
//! warning that divides exactly the two rates this wave's default divides — and
//! then writes a `sub_endpoints` entry with no discipline and a `node_paths`
//! entry with nothing but `output`. That is issue 1256's shape one layer
//! upstream of where W3 found it: a declaration legal to write, legal to
//! resolve, and dropped before any consumer can read it. Issue 1339.
//!
//! These are therefore TRIPWIRES, in the deliberate sense: two of them go RED
//! the day the model carries the field, and the red is the instruction to wire
//! `from_model` up. A test that only asserted the half that works would leave
//! the gap to be rediscovered.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test contract_queue_buffer_reaches_the_model`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::{
    entity_inventory::{EntityInventory, EntityKind},
    queue_depth::{NoDefault, RateMilliHz},
};
use ros_launch_manifest_model::SystemModel;

/// The resolver ACCEPTS all three facts — so the gap below is not a contract
/// that failed to parse.
///
/// The control for every assertion in this file. Without it, "the model does
/// not carry `buffer`" would be equally explained by a fixture the resolver
/// rejected, and the two have opposite remedies.
#[test]
fn the_resolver_accepts_a_contract_stating_buffer_and_both_rates() {
    let (_text, model) = resolve("queue");
    assert_eq!(
        model.structure.topics.len(),
        2,
        "the fixture's two topics resolved, so the file was read in full"
    );
    assert!(
        model.contracts.node_paths.contains_key("/listener/drain"),
        "the timer path resolved: {:?}",
        model.contracts.node_paths.keys().collect::<Vec<_>>()
    );
}

/// THE MEASUREMENT: layer 2 has BOTH rates and the discipline, and proves it by
/// performing this wave's own division.
///
/// The resolver's `queue-drain-rate` check reads the `buffer: queue`
/// subscriptions of a node, sums their producers' rates, and compares against
/// the node's timer path rate. That is `publish / drain` with a different
/// comparison on the end — so the facts are not merely present upstream, they
/// are already joined. Nothing about this wave needs inventing at layer 2; it
/// needs CARRYING.
#[test]
fn the_resolver_already_computes_the_division_this_wave_derives_from() {
    let (_text, model) = resolve("queue");
    let drain = model
        .meta
        .diagnostics
        .iter()
        .find(|d| d.contains("[queue-drain-rate]"))
        .unwrap_or_else(|| {
            panic!(
                "the resolver must reason about the queue's drain rate; it emitted:\n{}",
                model.meta.diagnostics.join("\n")
            )
        });
    // Both rates, named, in one upstream diagnostic.
    assert!(drain.contains("rate_hz (10)"), "{drain}");
    assert!(drain.contains("(50,"), "{drain}");
    assert!(drain.contains("buffer: queue"), "{drain}");
}

/// TRIPWIRE 1 — `SubContract` carries no `buffer`, so nano-ros cannot see the
/// discipline.
///
/// Asserted against the model's own YAML rather than against the typed struct,
/// because the typed struct HAS NO SUCH FIELD: there is nothing to call. When
/// `ros-launch-manifest` grows one and the resolver emits it, this key appears
/// and this test goes red — which is the moment `from_model`'s `buffer: None`
/// must become a read. Issue 1339.
#[test]
fn the_model_does_not_carry_the_buffer_discipline_yet() {
    let (text, _model) = resolve("queue");
    let raw: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&text).expect("the resolved model parses as YAML");
    let sub = raw
        .get("contracts")
        .and_then(|c| c.get("sub_endpoints"))
        .and_then(|s| s.get("/listener/chatter"))
        .expect("the subscriber endpoint has a contract entry");
    // The control: the entry is REAL and carries what the schema does model, so
    // an absent `buffer` is a missing FIELD rather than a missing entry.
    assert_eq!(
        sub.get("state").and_then(serde_yaml_ng::Value::as_bool),
        Some(true),
        "the same endpoint's `state: true` DID travel: {sub:?}"
    );
    assert!(
        sub.get("buffer").is_none(),
        "issue 1339 has closed -- `buffer` now reaches the SystemModel. Wire it into \
         `EntityInventory::from_model` (the `buffer: None` with this issue number beside \
         it) and delete this tripwire: {sub:?}"
    );
}

/// TRIPWIRE 2 — `PathContract` carries no trigger, so the DRAIN RATE is not in
/// the model either.
///
/// The fixture's drain path states `trigger: { timer: { rate_hz: 10 } }`. What
/// survives is a `node_paths` entry with `output` alone — which is why
/// `from_model` recovers the rate from what the timer PUBLISHES instead, the
/// same convention `mapper_input::pub_rate_hz` uses.
#[test]
fn the_model_does_not_carry_a_paths_trigger_rate_yet() {
    let (text, _model) = resolve("queue");
    let raw: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&text).expect("the resolved model parses as YAML");
    let path = raw
        .get("contracts")
        .and_then(|c| c.get("node_paths"))
        .and_then(|p| p.get("/listener/drain"))
        .expect("the drain path has an entry");
    assert!(
        path.get("output").is_some(),
        "the control: the path entry is real and carries its output: {path:?}"
    );
    assert!(
        path.get("trigger").is_none() && path.get("rate_hz").is_none(),
        "issue 1339 has closed -- a path's trigger now reaches the SystemModel. Read the \
         drain rate from it in `EntityInventory::from_model` instead of from the timer's \
         output promise, and delete this tripwire: {path:?}"
    );
}

/// THE ACCEPTANCE that does hold end to end: both rates reach the endpoint row,
/// from a real contract through the real resolver.
///
/// This is the half of the derivation that is live today, and it is what makes
/// the `NoDefault` this fixture produces INFORMATIVE. Without it every endpoint
/// in every image would report `NoPublishRate` — a reason that is true and
/// says nothing.
#[test]
fn both_rates_reach_the_endpoint_row_through_the_real_resolver() {
    let (_text, model) = resolve("queue");
    let inv = EntityInventory::from_model("fixture", &model).expect("the model describes wiring");
    let sub = inv
        .components()
        .iter()
        .flat_map(|c| c.declaration.entities())
        .find(|e| e.kind == EntityKind::Subscription)
        .expect("the listener subscribes to /chatter")
        .clone();
    assert_eq!(sub.name.as_deref(), Some("/chatter"));
    assert_eq!(
        sub.publish_rate,
        RateMilliHz::from_hz(50.0),
        "`topics./chatter.rate_hz: 50` is the channel's arrival rate"
    );
    assert_eq!(
        sub.drain_rate,
        RateMilliHz::from_hz(10.0),
        "the node's timer path publishes /status at min_rate_hz 10, so 10 Hz is the \
         drain rate this reader can see"
    );
}

/// ...and therefore the endpoint reports NO DEFAULT for the one fact that is
/// missing, naming it — RFC-0100 D9, acceptance 3.
///
/// The outcome is `NotAQueue` and NOT `NoPublishRate` or `NoDrainRate`, which
/// is the whole value of the test: with both rates live, the reason an author
/// reads points at the ONE thing that did not arrive rather than at a rate they
/// already stated. It also pins the derivation's inertness on today's toolchain
/// — a `queue` endpoint cannot exist, so no image's sizing can move.
#[test]
fn the_reason_names_the_discipline_because_both_rates_did_arrive() {
    let (_text, model) = resolve("queue");
    let inv = EntityInventory::from_model("fixture", &model).expect("the model describes wiring");
    let row = inv
        .queue_depth_defaults()
        .into_iter()
        .find(|r| r.topic == "/chatter")
        .expect("the subscription is reported");
    assert_eq!(
        row.outcome,
        Err(NoDefault::NotAQueue),
        "both rates arrived, so the missing fact is the discipline: {}",
        row.line()
    );
    assert!(row.publish_rate.is_some() && row.drain_rate.is_some());

    // No derived depth reaches the fragment, so no image's sizing moves.
    let cmake = inv.to_cmake();
    assert!(
        cmake.contains("set(NROS_ENTITY_DERIVED_DEPTHS \"\")\n"),
        "the derived list is published and EMPTY: {cmake}"
    );
    assert!(
        cmake.contains("set(NROS_ENTITY_DERIVED_DEPTH_COUNT 0)\n"),
        "{cmake}"
    );
    // ...and the diagnostics are silent, because no endpoint states a
    // discipline to contradict.
    assert!(inv.buffer_diagnostics().is_empty());
}

/// Resolve `launch/<stem>.launch.xml` (with its `<stem>.contract.yaml`
/// sidecar) through the pinned resolver, by ABSOLUTE path (issue 0285).
///
/// Returns the model's raw TEXT beside the parsed value, because two of the
/// assertions here are about keys the typed model has no field for.
fn resolve(stem: &str) -> (String, SystemModel) {
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
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/queue_buffer");
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
    let parsed = SystemModel::from_yaml_str(&text).expect("the resolved model parses");
    (text, parsed)
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
/// files live in `$project/tmp/`, not the system temp dir).
fn temp_output(repo: &Path, name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = repo.join("tmp").join(format!(
        "queue-buffer-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
