//! phase-462 W2 -- the contract's `on_violation` and the tier table's
//! `deadline_policy` are two statements about ONE fact, so a divergence is a
//! build error naming both sites (phase-454 W7's shape, in
//! [`crate::qos_agreement`]).
//!
//! # What the contract actually carries
//!
//! The phase document describes `on_violation` as a word out of `warn | skip |
//! fault` sitting on a path. The model this tree pins (ros-launch-manifest
//! v0.1.40) does not spell it that way, and this module lowers what is THERE:
//!
//! - `contracts.node_paths.<ref>.miss.action` -- `continue | skip_next |
//!   abort`, the sched crate's [`MapperMissAction`]. This is the per-path
//!   reaction to a missed deadline, and it is the field that maps onto the
//!   executor's `DeadlineAction` one to one.
//! - `contracts.sub_endpoints.<ref>.on_violation` -- `{ on, reaction,
//!   within_ms, mechanism }`: not a word but a POINTER to the node path that
//!   implements the reaction. Its action is therefore the action of the path
//!   it names, and an `on_violation` whose `reaction` names no node path is a
//!   contract that says a reaction is owed and cannot say what runs it, which
//!   refuses here.
//!
//! Both roads end in the same three words, which is the executor's own
//! vocabulary (`DeadlineAction::from_tier_str` in
//! `nros-node/src/executor/sched_context.rs`): `warn`, `skip`, `fault`. The
//! two crates cannot share a type -- the executor is `no_std` and knows
//! nothing about the model -- so [`ViolationAction::as_tier_str`] is the
//! agreed spelling and both sides have a test pinning it.
//!
//! # Why the tier and not the endpoint
//!
//! What the executor DOES on a missed deadline is a property of the
//! SchedContext the callback runs under, not of the endpoint: one SC, one
//! reaction. So the contract's action lands on the tier that runs the
//! declaring node's callbacks (`execution.bindings`), and two nodes on one
//! tier declaring different actions is a refusal rather than a silent
//! strongest-wins -- the image would run one of the two contracts and not say
//! which.
//!
//! # When this abstains
//!
//! When no contract declares an action at all, or when a tier has no node
//! bound to it: then there is one producer (the tier table) and nothing to
//! disagree with, and an authored `deadline_policy` stands.

use std::{collections::BTreeMap, fmt};

use ros_launch_manifest_model::SystemModel;
use ros_launch_manifest_sched::MapperMissAction;

/// What the executor does when the reaction this contract owes is triggered.
///
/// `Ignore` has no variant on purpose: it is the ABSENCE of a declaration, and
/// a contract that declares an `on_violation` has by definition not asked to
/// ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationAction {
    /// Report the violation on the drain and carry on.
    Warn,
    /// Report, and skip the offending SchedContext's remaining callbacks for
    /// the rest of the cycle.
    Skip,
    /// Report, and invoke the executor's fault hook.
    Fault,
}

impl ViolationAction {
    /// The spelling the baked tier table carries and
    /// `DeadlineAction::from_tier_str` reads. The two crates share no type, so
    /// this string IS the interface.
    pub const fn as_tier_str(self) -> &'static str {
        match self {
            ViolationAction::Warn => "warn",
            ViolationAction::Skip => "skip",
            ViolationAction::Fault => "fault",
        }
    }

    /// The tier table's own vocabulary. `ignore` is a declaration of NO
    /// reaction, so it reads as `None` here and never disagrees with a
    /// contract that declares one -- it is the default the executor already
    /// has.
    pub fn from_tier_str(s: &str) -> Option<Self> {
        match s {
            "warn" => Some(ViolationAction::Warn),
            "skip" => Some(ViolationAction::Skip),
            "fault" => Some(ViolationAction::Fault),
            _ => None,
        }
    }

    /// The contract's own vocabulary, on a node path's `miss.action`.
    ///
    /// `continue` is NOT `ignore`: declaring a miss policy at all means the
    /// system must know the deadline was missed (`MapperMiss::requires_detection`
    /// says so on the rlm side), and knowing it on target means reporting it.
    pub const fn from_miss_action(a: MapperMissAction) -> Self {
        match a {
            MapperMissAction::Continue => ViolationAction::Warn,
            MapperMissAction::SkipNext => ViolationAction::Skip,
            MapperMissAction::Abort => ViolationAction::Fault,
        }
    }
}

impl fmt::Display for ViolationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_tier_str())
    }
}

/// One declared action and the contract ref that declared it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSite {
    pub action: ViolationAction,
    /// The contract ref a message names: a node-path ref, or
    /// `"<endpoint> -> <reaction path>"` when an `on_violation` pointed at it.
    pub site: String,
}

/// A tier whose contract-declared action and authored `deadline_policy`
/// disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    pub tier: String,
    pub contract: ActionSite,
    pub declared: String,
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "tier `{}`:", self.tier)?;
        writeln!(
            f,
            "  contract   : `{}` states `{}`",
            self.contract.site, self.contract.action
        )?;
        writeln!(
            f,
            "  tier table : `[tiers.{}] deadline_policy = \"{}\"`",
            self.tier, self.declared
        )?;
        write!(
            f,
            "  They are two statements about one fact: the tier string is what the entry bakes \
             into the SchedContext and the contract is what every other consumer reads, so \
             honouring the tier silently would run a reaction the contract does not describe. \
             Change one of them."
        )
    }
}

/// Every way this lowering can refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViolationAgreementError {
    /// `on_violation.reaction` names no node path in the model.
    UnknownReaction { endpoint: String, reaction: String },
    /// Two nodes on one tier declare different actions. One SchedContext has
    /// one reaction, so the bake cannot honour both.
    TierSplit {
        tier: String,
        first: ActionSite,
        second: ActionSite,
    },
    /// The contract and the tier table disagree.
    Divergent(Vec<Disagreement>),
}

impl fmt::Display for ViolationAgreementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViolationAgreementError::UnknownReaction { endpoint, reaction } => write!(
                f,
                "contract: `sub_endpoints.{endpoint}.on_violation.reaction` names `{reaction}`, \
                 which is not a node path in this model. The contract says a reaction is owed \
                 and cannot say what runs it, so the image would carry the promise and not the \
                 behaviour."
            ),
            ViolationAgreementError::TierSplit {
                tier,
                first,
                second,
            } => write!(
                f,
                "tier `{tier}` runs callbacks whose contracts declare two different violation \
                 actions: `{}` states `{}` and `{}` states `{}`. A SchedContext has ONE deadline \
                 reaction, so the bake would run one contract and not say which. Split the tier, \
                 or state the same action on both.",
                first.site, first.action, second.site, second.action
            ),
            ViolationAgreementError::Divergent(ds) => {
                writeln!(
                    f,
                    "{} tier(s) disagree between the launch contract's `on_violation` and the \
                     tier table's `deadline_policy`. The contract is the authoritative producer: \
                     it is what play_launch enforces on the Linux side of the same system, and \
                     an image that reacts differently on target is the divergence this check \
                     exists to refuse.",
                    ds.len()
                )?;
                for d in ds {
                    writeln!(f)?;
                    writeln!(f, "{d}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ViolationAgreementError {}

/// The node FQN a contract ref belongs to: `/ctrl/control_node/tick` ->
/// `/ctrl/control_node`. Same rule as `qos_agreement::node_of`.
fn node_of(reference: &str) -> &str {
    reference
        .rsplit_once('/')
        .map(|(n, _)| n)
        .unwrap_or(reference)
}

/// The node a `execution.bindings` key names. The key is `"<node FQN>"` or
/// `"<node FQN>/<callback group>"`, and a node FQN contains slashes, so the
/// model's own node list decides which of the two it is.
fn bound_node<'a>(key: &'a str, model: &SystemModel) -> &'a str {
    if model.structure.nodes.contains_key(key) {
        key
    } else {
        node_of(key)
    }
}

/// Every node that declares a violation action, and where it declared it.
///
/// A node declares one through its own paths' `miss.action`, or by being the
/// node whose path an endpoint's `on_violation` points at. The strongest
/// action wins within a node (`fault` > `skip` > `warn`): they are reactions
/// to the same SchedContext missing a deadline, and containing the worst case
/// is the only reading that honours both.
pub fn node_actions(
    model: &SystemModel,
) -> Result<BTreeMap<String, ActionSite>, ViolationAgreementError> {
    let mut out: BTreeMap<String, ActionSite> = BTreeMap::new();
    let mut record = |node: &str, candidate: ActionSite| {
        out.entry(node.to_string())
            .and_modify(|held| {
                if candidate.action > held.action {
                    *held = candidate.clone();
                }
            })
            .or_insert(candidate);
    };

    for (path_ref, path) in &model.contracts.node_paths {
        let Some(action) = path.miss.as_ref().and_then(|m| m.action) else {
            continue;
        };
        record(
            node_of(path_ref),
            ActionSite {
                action: ViolationAction::from_miss_action(action),
                site: path_ref.clone(),
            },
        );
    }

    for (ep_ref, sub) in &model.contracts.sub_endpoints {
        let Some(on_violation) = sub.on_violation.as_ref() else {
            continue;
        };
        let reaction = &on_violation.reaction;
        let Some((path_ref, path)) = model
            .contracts
            .node_paths
            .get_key_value(reaction)
            // The contract addresses a path by its ref; a leading slash is the
            // one spelling that legitimately differs between the authoring
            // side and the resolved model (see the resolver's `fqn`).
            .or_else(|| {
                model
                    .contracts
                    .node_paths
                    .get_key_value(reaction.trim_start_matches('/'))
            })
        else {
            return Err(ViolationAgreementError::UnknownReaction {
                endpoint: ep_ref.clone(),
                reaction: reaction.clone(),
            });
        };
        // A declared reaction with no `miss.action` still has to be SEEN --
        // that is what `on_violation` asks for -- so it lowers to `warn`.
        let action = path
            .miss
            .as_ref()
            .and_then(|m| m.action)
            .map(ViolationAction::from_miss_action)
            .unwrap_or(ViolationAction::Warn);
        record(
            node_of(path_ref),
            ActionSite {
                action,
                site: format!("{ep_ref} -> {path_ref}"),
            },
        );
    }
    Ok(out)
}

/// The action each tier must run, from the contracts of the nodes bound to it.
///
/// Tiers with no contracted node are absent from the map: the tier table's own
/// `deadline_policy` is then the only statement, and it stands.
pub fn tier_actions(
    model: &SystemModel,
) -> Result<BTreeMap<String, ActionSite>, ViolationAgreementError> {
    let by_node = node_actions(model)?;
    let mut out: BTreeMap<String, ActionSite> = BTreeMap::new();
    for (key, tier) in &model.execution.bindings {
        let Some(declared) = by_node.get(bound_node(key, model)) else {
            continue;
        };
        match out.get(tier) {
            None => {
                out.insert(tier.clone(), declared.clone());
            }
            Some(held) if held.action == declared.action => {}
            Some(held) => {
                return Err(ViolationAgreementError::TierSplit {
                    tier: tier.clone(),
                    first: held.clone(),
                    second: declared.clone(),
                });
            }
        }
    }
    Ok(out)
}

/// The tier table this bake must emit: tier name -> the `deadline_policy`
/// string, taken from the contract and checked against whatever the tier table
/// authored.
///
/// This is the whole lowering. `plan_from_model` applies the result to the
/// `TierDef`s it builds, so the baked entry's `run_tiers` / SchedContext rows
/// carry the contract's word and the executor reacts to it -- rather than the
/// word staying in a YAML file, which is the state phase-462 exists to end.
pub fn contract_deadline_policies(
    model: &SystemModel,
) -> Result<BTreeMap<String, String>, ViolationAgreementError> {
    let actions = tier_actions(model)?;
    let mut divergent = Vec::new();
    let mut out = BTreeMap::new();
    for (tier, contract) in actions {
        let declared = model
            .execution
            .tiers
            .get(&tier)
            .and_then(|t| t.deadline_policy.as_deref());
        match declared {
            // `ignore`, or an unreadable word, is not a competing statement:
            // the executor's default IS ignore, so a contract that declares a
            // reaction simply supplies what was missing.
            Some(d) if ViolationAction::from_tier_str(d) == Some(contract.action) => {}
            Some(d) if ViolationAction::from_tier_str(d).is_some() => {
                divergent.push(Disagreement {
                    tier: tier.clone(),
                    contract: contract.clone(),
                    declared: d.to_string(),
                });
                continue;
            }
            _ => {}
        }
        out.insert(tier, contract.action.as_tier_str().to_string());
    }
    if !divergent.is_empty() {
        return Err(ViolationAgreementError::Divergent(divergent));
    }
    Ok(out)
}

/// The agreement gate, for the bake road that only needs a verdict.
pub fn check_model(model: &SystemModel) -> Result<(), ViolationAgreementError> {
    contract_deadline_policies(model).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model with one contracted node on one tier. `miss` and `on_violation`
    /// are filled by the callers below.
    fn model_yaml(path_miss: &str, on_violation: &str, tier_policy: &str) -> String {
        format!(
            "meta:\n  version: 1\nstructure:\n  scopes:\n    demo.launch.xml:\n      \
             file: demo.launch.xml\n  nodes:\n    /ctrl/control_node:\n      \
             scope: demo.launch.xml\n      \
             pkg: demo\n      exec: control_node\n      node_name: control_node\n      \
             namespace: /ctrl\ncontracts:\n  node_paths:\n    /ctrl/control_node/tick:\n      \
             output: [/ctrl/control_node/cmd]\n      max_latency_ms: 20.0\n{path_miss}\
             \n  sub_endpoints:\n    /ctrl/control_node/scan:\n      max_age_ms: 100.0\n\
             {on_violation}\nexecution:\n  bindings:\n    /ctrl/control_node: rt\n  tiers:\n    \
             rt:\n      class: real_time\n{tier_policy}"
        )
    }

    fn parse(yaml: &str) -> SystemModel {
        SystemModel::from_yaml_str(yaml).expect("fixture parses")
    }

    const MISS_ABORT: &str = "      miss:\n        action: abort\n";
    const OV_TICK: &str = "      on_violation:\n        on: [omission]\n        \
                           reaction: /ctrl/control_node/tick\n";

    #[test]
    fn a_paths_miss_action_lowers_to_the_tier_string() {
        let m = parse(&model_yaml(MISS_ABORT, "", ""));
        let policies = contract_deadline_policies(&m).expect("no disagreement");
        assert_eq!(policies.get("rt").map(String::as_str), Some("fault"));
    }

    /// The `on_violation` road: the endpoint points at a path, and the path's
    /// own miss policy is the action.
    #[test]
    fn on_violation_takes_the_action_of_the_path_it_names() {
        let m = parse(&model_yaml(MISS_ABORT, OV_TICK, ""));
        let by_node = node_actions(&m).expect("resolves");
        let site = by_node.get("/ctrl/control_node").expect("declared");
        assert_eq!(site.action, ViolationAction::Fault);
    }

    /// An `on_violation` with no miss policy on its reaction still has to be
    /// SEEN: `warn`, never silence.
    #[test]
    fn a_reaction_without_a_miss_policy_still_reports() {
        let m = parse(&model_yaml("", OV_TICK, ""));
        let policies = contract_deadline_policies(&m).expect("no disagreement");
        assert_eq!(policies.get("rt").map(String::as_str), Some("warn"));
    }

    /// The negative control the phase asks for: a contract and a tier string
    /// that disagree refuse, naming both.
    #[test]
    fn a_tier_string_that_disagrees_refuses_naming_both() {
        let m = parse(&model_yaml(
            MISS_ABORT,
            OV_TICK,
            "      deadline_policy: warn\n",
        ));
        let err = contract_deadline_policies(&m).expect_err("must refuse");
        let text = err.to_string();
        assert!(text.contains("fault"), "names the contract's word: {text}");
        assert!(text.contains("warn"), "names the tier's word: {text}");
        assert!(text.contains("rt"), "names the tier: {text}");
    }

    /// The same word on both sides is agreement, not a repeat offence.
    #[test]
    fn the_same_word_on_both_sides_agrees() {
        let m = parse(&model_yaml(
            MISS_ABORT,
            "",
            "      deadline_policy: fault\n",
        ));
        assert!(check_model(&m).is_ok());
    }

    /// `ignore` is the executor's default, not a competing claim.
    #[test]
    fn ignore_is_not_a_competing_statement() {
        let m = parse(&model_yaml(
            MISS_ABORT,
            "",
            "      deadline_policy: ignore\n",
        ));
        let policies = contract_deadline_policies(&m).expect("no disagreement");
        assert_eq!(policies.get("rt").map(String::as_str), Some("fault"));
    }

    #[test]
    fn a_reaction_naming_no_path_refuses() {
        let m = parse(&model_yaml(
            "",
            "      on_violation:\n        on: [omission]\n        reaction: /ctrl/control_node/nope\n",
            "",
        ));
        let err = node_actions(&m).expect_err("must refuse");
        assert!(matches!(
            err,
            ViolationAgreementError::UnknownReaction { .. }
        ));
        assert!(err.to_string().contains("nope"));
    }

    /// A model with no `miss` and no `on_violation` states nothing, and the
    /// tier table stands alone.
    #[test]
    fn a_contract_that_declares_nothing_abstains() {
        let m = parse(&model_yaml("", "", "      deadline_policy: skip\n"));
        assert!(
            contract_deadline_policies(&m)
                .expect("no refusal")
                .is_empty()
        );
    }

    /// The three words are the executor's, and `DeadlineAction::from_tier_str`
    /// is the reader. `nros-node` pins the same three in
    /// `sched_context.rs::the_contract_vocabulary_is_the_tier_vocabulary`.
    #[test]
    fn the_tier_spelling_round_trips() {
        for a in [
            ViolationAction::Warn,
            ViolationAction::Skip,
            ViolationAction::Fault,
        ] {
            assert_eq!(ViolationAction::from_tier_str(a.as_tier_str()), Some(a));
        }
        assert_eq!(ViolationAction::from_tier_str("ignore"), None);
    }
}
