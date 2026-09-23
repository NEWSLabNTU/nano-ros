//! phase-462 W2 -- `on_violation` lowers to something the executor DOES.
//!
//! The contract's safety vocabulary stopped at Linux: `on_violation` was read
//! by rlm and by play_launch's reaction engine, and by NOTHING in this
//! repository (the phase-462 table's "nothing" column, verified by grep over
//! the CLI and IR sources). This gate is the claim that it no longer does, and
//! it follows the word the whole way:
//!
//! 1. the contract states a reaction (`sub_endpoints.*.on_violation`, pointing
//!    at a node path, whose `miss.action` says what happens);
//! 2. `violation_agreement` lowers it to the tier that runs that path's
//!    callbacks, refusing when a tier table states a different word;
//! 3. `resolve_tiers` carries it into the `ResolvedTier` the entry emitters
//!    bake -- `deadline_policy`, the string
//!    `SchedContext::from_tier_policy` turns into a `DeadlineAction`
//!    (`nros-node/src/executor/sched_context.rs`), which is what the executor
//!    runs on a missed deadline: report, skip the SC's siblings, or call the
//!    fault hook.
//!
//! The NEGATIVE CONTROL is the third assertion of
//! [`the_contracts_word_reaches_the_baked_tier`]: the same fixture resolved
//! WITHOUT the lowering applied -- which is the tree before this wave --
//! resolves to `deadline_policy: None`, so the executor's action is `Ignore`
//! and the declared `abort` is dropped on the floor. The test prints both
//! resolutions, so a regression names the field that stopped moving.
//!
//! The silence half is the same `on_violation`'s other target and is checked
//! here as the LEASE the contract states (`max_age_ms`), because a
//! subscription that takes nothing for that long is the violation the age rule
//! cannot see. The rule itself is exercised against a real
//! `SubMonitorCell` in `nros-node`'s `executor::monitor` tests
//! (`a_silent_subscription_fires_once_until_it_is_fed`,
//! `silence_needs_a_contract_a_gap_and_a_clock`); this crate cannot build an
//! executor without the optional `trigger-test` dependency set, and a gate
//! that compiles to nothing under the command the phase names would be worse
//! than one that states where the runtime half is proved.

use std::collections::{BTreeMap, BTreeSet};

use nros_orchestration_ir::{
    CallbackGroupDecl, TierDef, resolve_tiers, tier_from_model,
    violation_agreement::{ViolationAction, contract_deadline_policies, node_actions},
};
use ros_launch_manifest_model::SystemModel;

/// One contracted node, one declared tier, one reaction path.
///
/// `MISS` and `TIER_POLICY` are substituted so the three cases differ by
/// exactly the line under test.
fn fixture(miss: &str, tier_policy: &str) -> String {
    format!(
        r#"
meta:
  version: 1
structure:
  scopes:
    island.launch.xml:
      file: island.launch.xml
  nodes:
    /perception/detector:
      scope: island.launch.xml
      pkg: island
      exec: detector
      node_name: detector
      namespace: /perception
  topics:
    /sensing/scan:
      type: sensor_msgs/msg/LaserScan
      publishers: []
      subscribers: [/perception/detector/scan]
contracts:
  node_paths:
    /perception/detector/proc:
      input: [/perception/detector/scan]
      output: [/perception/detector/objects]
      max_latency_ms: 20.0
{miss}
  sub_endpoints:
    /perception/detector/scan:
      max_age_ms: 100.0
      on_violation:
        on: [omission]
        reaction: /perception/detector/proc
execution:
  bindings:
    /perception/detector: rt
  tiers:
    rt:
      class: real_time
{tier_policy}      posix:
        priority: 20
        stack_bytes: 16384
"#
    )
}

const MISS_ABORT: &str = "      miss:\n        action: abort\n";

fn model(yaml: &str) -> SystemModel {
    SystemModel::from_yaml_str(yaml).expect("the fixture parses as a SystemModel")
}

/// The tier table a bake builds from the model, with and without this wave's
/// lowering applied -- `plan_from_model`'s two lines, so the gate exercises
/// the same code the entry bake runs.
fn tier_defs(m: &SystemModel) -> BTreeMap<String, TierDef> {
    m.execution
        .tiers
        .iter()
        .map(|(name, t)| (name.clone(), tier_from_model(t, "posix")))
        .collect()
}

fn resolved_policy(tiers: &BTreeMap<String, TierDef>) -> Option<String> {
    let groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::from([(
        "detector".to_string(),
        vec![CallbackGroupDecl {
            id: "proc".to_string(),
            r#type: "MutuallyExclusive".to_string(),
            tier: "rt".to_string(),
        }],
    )]);
    let names: BTreeSet<&str> = BTreeSet::from(["detector"]);
    let table = resolve_tiers(tiers, &[], &names, &groups, "posix").expect("the tier resolves");
    table
        .tiers
        .iter()
        .find(|t| t.name == "rt")
        .expect("the declared tier is in the table")
        .deadline_policy
        .clone()
}

/// The whole chain: a contract word becomes the tier string the executor
/// lowers to a `DeadlineAction`.
#[test]
fn the_contracts_word_reaches_the_baked_tier() {
    let m = model(&fixture(MISS_ABORT, ""));

    // 1. The contract states a reaction, and it is `abort`.
    let by_node = node_actions(&m).expect("the reaction path resolves");
    assert_eq!(
        by_node.get("/perception/detector").map(|a| a.action),
        Some(ViolationAction::Fault),
        "`abort` on the reaction path is the executor's fault action"
    );

    // 2. It lands on the tier that runs that node's callbacks.
    let policies = contract_deadline_policies(&m).expect("no disagreement");
    assert_eq!(policies.get("rt").map(String::as_str), Some("fault"));

    // 3. And it reaches the resolved tier the entry bakes.
    let mut tiers = tier_defs(&m);
    let before = resolved_policy(&tiers);
    for (tier, action) in &policies {
        if let Some(def) = tiers.get_mut(tier) {
            def.deadline_policy = Some(action.clone());
        }
    }
    let after = resolved_policy(&tiers);
    println!(
        "on_violation lowering: resolved tier `rt` deadline_policy {before:?} -> {after:?} \
         (negative control: {before:?} is DeadlineAction::Ignore -- the declared `abort` \
         dropped on the floor)"
    );
    assert_eq!(
        before, None,
        "the negative control: without the lowering the tier states nothing, so the \
         executor's action is Ignore"
    );
    assert_eq!(
        after.as_deref(),
        Some("fault"),
        "the baked tier carries the contract's word; `SchedContext::from_tier_policy` \
         turns exactly this string into `DeadlineAction::Fault`"
    );
}

/// The refusal the phase asks for: a contract and a tier string that disagree
/// stop the bake, naming both.
#[test]
fn a_tier_table_that_contradicts_the_contract_refuses() {
    let m = model(&fixture(MISS_ABORT, "      deadline_policy: warn\n"));
    let err = contract_deadline_policies(&m)
        .expect_err("two statements about one fact must not both be honoured")
        .to_string();
    println!("negative control (disagreement) printed:\n{err}");
    for expected in ["rt", "fault", "warn", "/perception/detector/proc"] {
        assert!(
            err.contains(expected),
            "the refusal must name `{expected}`:\n{err}"
        );
    }
}

/// The silence half: the lease the executor holds the input against is the
/// endpoint's own `max_age_ms`, and it needs no second declaration.
///
/// `silence-runtime` is the rule id (`nros-diagnostics::RULE_SILENCE`); the
/// runtime that fires it is `nros-node`'s `check_age` / `check_silence`.
#[test]
fn the_silence_lease_is_the_declared_age_bound() {
    let m = model(&fixture(MISS_ABORT, ""));
    let ep = m
        .contracts
        .sub_endpoints
        .get("/perception/detector/scan")
        .expect("the fixture's contracted subscription");
    assert!(
        ep.on_violation.is_some(),
        "the endpoint declares what is owed when its assumption is violated"
    );
    assert_eq!(
        ep.max_age_ms,
        Some(100.0),
        "and the window the executor leases it against -- data that never arrives is \
         older than `max_age_ms` by the same clock"
    );
}

/// A model that declares no reaction lowers nothing and refuses nothing: the
/// tier table is then the only statement, and it stands.
#[test]
fn a_contract_without_on_violation_changes_no_tier() {
    let mut m = model(&fixture(MISS_ABORT, "      deadline_policy: skip\n"));
    m.contracts.node_paths.clear();
    m.contracts.sub_endpoints.clear();
    let policies = contract_deadline_policies(&m).expect("nothing to disagree with");
    assert!(policies.is_empty());
    assert_eq!(
        resolved_policy(&tier_defs(&m)).as_deref(),
        Some("skip"),
        "an authored tier policy survives a contract that says nothing"
    );
}
