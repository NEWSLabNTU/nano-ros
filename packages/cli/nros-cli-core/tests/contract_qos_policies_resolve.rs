//! phase-454 W3 (issue 1256) -- all four QoS policies survive resolution.
//!
//! The unit tests in `entity_inventory.rs` feed `EntityInventory::from_model` a
//! hand-written model. This one produces the model the way a build does: the
//! pinned `nros-launch-resolve` over a launch file and its contract sidecar,
//! exactly as `nros sync` would. Both halves have to work for a declaration to
//! reach the build, and as with the publisher depth in W2, only one of them was
//! ever broken -- `QosDecl` has carried `reliability`, `durability` and
//! `history` all along and `from_model` read `depth` and dropped the rest.
//!
//! Two fixtures, because the two outcomes are different facts:
//!
//! * `policies` -- a four-policy endpoint on each side of a topic, and a silent
//!   pair beside it. The round trip.
//! * `keep_all` -- `history: keep_all` with a `depth: 1` beside it, which is
//!   the shape that was priced at 1 for a queue with no bound. The refusal.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test contract_qos_policies_resolve`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::entity_inventory::{
    ALL_QOS_POLICY_KINDS, DeclaredDepths, EntityInventory, EntityKind, QosPolicyKind,
};
use ros_launch_manifest_model::SystemModel;

/// One endpoint's row as an assertion reads it: `(kind, topic, reliability,
/// durability, history)`, each policy in the CONTRACT's own spelling and
/// `None` where the endpoint stated nothing.
type PolicyRow<'a> = (
    &'a str,
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
    Option<&'a str>,
);

/// The resolver carries all four policies into the model -- the half that was
/// never broken, asserted anyway so a failure here cannot be misread as ours.
#[test]
fn the_resolver_carries_every_stated_policy_into_the_model() {
    let m = resolve("policies");
    let p = m
        .contracts
        .pub_endpoints
        .get("/talker/chatter")
        .and_then(|c| c.qos.as_ref())
        .expect("the publisher states a qos block");
    assert_eq!(p.depth, Some(8));
    assert_eq!(p.reliability.as_deref(), Some("reliable"));
    assert_eq!(p.durability.as_deref(), Some("transient_local"));
    assert_eq!(p.history.as_deref(), Some("keep_last"));

    let s = m
        .contracts
        .sub_endpoints
        .get("/listener/chatter")
        .and_then(|c| c.qos.as_ref())
        .expect("the subscriber states a qos block");
    assert_eq!(s.depth, Some(3));
    assert_eq!(s.reliability.as_deref(), Some("best_effort"));
    assert_eq!(s.durability.as_deref(), Some("volatile"));
    assert_eq!(s.history.as_deref(), Some("keep_last"));
}

/// THE ACCEPTANCE: a contract stating all four policies round-trips -- parsed,
/// carried, and readable by a consumer.
///
/// "Readable by a consumer" is asserted against the two surfaces a consumer
/// actually reads: the typed view (`declared_qos`) and the CMake projection the
/// build includes.
#[test]
fn a_four_policy_contract_reaches_the_inventory_and_both_projections() {
    let inv = EntityInventory::from_model("fixture", &resolve("policies"))
        .expect("the model describes wiring");

    // 1. The typed view.
    let rows = inv.declared_qos();
    let rows = rows.rows().expect("a composed inventory resolves");
    let seen: Vec<PolicyRow<'_>> = rows
        .iter()
        .map(|r| {
            (
                r.kind.tag(),
                r.topic.as_str(),
                r.spelling(QosPolicyKind::Reliability),
                r.spelling(QosPolicyKind::Durability),
                r.spelling(QosPolicyKind::History),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            (
                "publisher",
                "/chatter",
                Some("reliable"),
                Some("transient_local"),
                Some("keep_last"),
            ),
            (
                "subscription",
                "/chatter",
                Some("best_effort"),
                Some("volatile"),
                Some("keep_last"),
            ),
        ],
        "the two sides state different profiles, and each must land on its own row"
    );
    // `/quiet` said nothing, on both sides, for all three policies. It is not
    // a row and it IS a count -- the distinction the whole view exists for.
    for policy in ALL_QOS_POLICY_KINDS {
        for kind in [EntityKind::Subscription, EntityKind::Publisher] {
            assert_eq!(
                inv.declared_qos().undeclared(*policy, kind),
                Some(1),
                "one silent {} for {}",
                kind.tag(),
                policy.tag()
            );
        }
    }

    // 2. The depth is still there and still split by kind (W2's invariant).
    let DeclaredDepths::Resolved { rows, .. } = inv.declared_depths() else {
        panic!("no endpoint here says keep_all, so the depth table resolves");
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

    // 3. The CMake projection -- the transport a build actually includes.
    let cmake = inv.to_cmake();
    for want in [
        "set(NROS_ENTITY_DECLARED_QOS_STATUS \"resolved\")\n",
        "set(NROS_ENTITY_DECLARED_RELIABILITY \"std_msgs/msg/Int32|/chatter=best_effort\")\n",
        "set(NROS_ENTITY_DECLARED_RELIABILITY_PUBLISHER \
         \"std_msgs/msg/Int32|/chatter=reliable\")\n",
        "set(NROS_ENTITY_DECLARED_DURABILITY \"std_msgs/msg/Int32|/chatter=volatile\")\n",
        "set(NROS_ENTITY_DECLARED_DURABILITY_PUBLISHER \
         \"std_msgs/msg/Int32|/chatter=transient_local\")\n",
        "set(NROS_ENTITY_DECLARED_HISTORY \"std_msgs/msg/Int32|/chatter=keep_last\")\n",
        "set(NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_SUBSCRIPTION 1)\n",
        "set(NROS_ENTITY_UNDECLARED_DURABILITY_COUNT_PUBLISHER 1)\n",
    ] {
        assert!(cmake.contains(want), "missing `{want}` in:\n{cmake}");
    }
}

/// THE ACCEPTANCE for the defect: `history: keep_all` REFUSES, and the refusal
/// names the endpoint.
///
/// The fixture states `depth: 1` beside the `keep_all`, which is the pair that
/// was silently priced at 1. A KEEP_ALL queue has no static bound, so pricing
/// from that 1 budgets one sample for a queue DDS will let grow -- an UNDER-size,
/// which lands as `NodeError::BufferTooSmall` at a registration this table had
/// passed. There is no number to fall back to, so there is no fallback.
#[test]
fn a_keep_all_endpoint_refuses_the_depth_table_and_names_itself() {
    let inv = EntityInventory::from_model("fixture", &resolve("keep_all"))
        .expect("the model describes wiring");

    let DeclaredDepths::Refused { reason } = inv.declared_depths() else {
        panic!("a keep_all endpoint has no static bound, so the depth table must refuse");
    };
    assert!(
        reason.contains("/chatter"),
        "the refusal must NAME the endpoint, or nobody can act on it: {reason}"
    );
    assert!(reason.contains("keep_all"), "{reason}");
    assert!(
        reason.contains("NO STATIC BOUND"),
        "it must say WHY, not just that it refused: {reason}"
    );
    assert!(
        reason.contains("keep_last"),
        "and name the remedy (RFC-0065 D2): {reason}"
    );

    // And NO number survives. The `depth: 5` on the other subscription is a
    // real declaration, and publishing it here would be the partial table that
    // sizes an image from a subset of itself.
    let cmake = inv.to_cmake();
    assert!(
        cmake.contains("set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"refused\")\n"),
        "{cmake}"
    );
    assert!(
        !cmake.contains("set(NROS_ENTITY_DECLARED_DEPTHS "),
        "a refused table publishes no list -- not even an empty one, which reads as \
         \"nobody declared\": {cmake}"
    );
    assert!(
        !cmake.contains("=5"),
        "the sibling endpoint's depth must not leak out of a refused table: {cmake}"
    );
}

/// ...and the refusal is PER FACT (RFC-0100 D6): it kills the depth-derived
/// numbers and NOTHING else.
///
/// This is the half that is easy to get wrong, and D6 says so outright: *"a
/// `keep_all` subscription says nothing about Cyclone's type table, and a global
/// refusal would degrade it anyway."* The entity counts, the subscribed-type
/// set and the policy table itself all stay resolved -- including the `keep_all`
/// row, because the statement is perfectly well declared and a consumer that
/// reads history (an XRCE `STREAM_HISTORY`, a Cyclone resource limit) needs it.
#[test]
fn the_keep_all_refusal_degrades_no_fact_it_says_nothing_about() {
    let inv = EntityInventory::from_model("fixture", &resolve("keep_all"))
        .expect("the model describes wiring");

    assert!(
        inv.derive().knobs().is_some(),
        "an entity COUNT does not depend on any queue depth"
    );
    assert!(
        inv.subscribed_types().types().is_some(),
        "a payload class is a property of the TYPE, not of the history policy"
    );

    let qos = inv.declared_qos();
    let rows = qos
        .rows()
        .expect("the policy table resolves -- keep_all is a statement, not a gap");
    assert_eq!(
        rows.iter()
            .map(|r| (r.topic.as_str(), r.spelling(QosPolicyKind::History)))
            .collect::<Vec<_>>(),
        vec![
            ("/chatter", Some("keep_all")),
            ("/quiet", Some("keep_last")),
        ],
        "the keep_all row must SURVIVE; refusing it would be the global refusal D6 forbids"
    );

    let cmake = inv.to_cmake();
    assert!(
        cmake.contains("set(NROS_ENTITY_DECLARED_QOS_STATUS \"resolved\")\n"),
        "{cmake}"
    );
    assert!(
        cmake.contains(
            "set(NROS_ENTITY_DECLARED_HISTORY \
             \"std_msgs/msg/Int32|/chatter=keep_all;std_msgs/msg/Int32|/quiet=keep_last\")\n"
        ),
        "{cmake}"
    );
    assert!(
        cmake.contains("set(NROS_DERIVED_EXECUTOR_MAX_CBS "),
        "the slot count is unaffected: {cmake}"
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
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/qos_policies");
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
        "qos-policies-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
