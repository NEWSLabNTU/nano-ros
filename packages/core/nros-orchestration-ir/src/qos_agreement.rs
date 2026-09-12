//! phase-454 W7 (RFC-0100 D8) — the launch contract and `qos_overrides.*` are
//! two statements about ONE fact, so any divergence is a build error naming
//! both sites.
//!
//! `qos_overrides.<topic>.<role>.<policy>` is ROS 2's own mechanism for
//! overriding QoS declared in code, delivered as ordinary node parameters — so
//! it reaches a build from launch XML `<param>`, a params YAML, or
//! `system.toml [[component]] params`. It feeds the baked RUNTIME table
//! ([`crate::qos_override::lower_all`] → the C/C++ entry emitters and the
//! `nros::main!` proc-macro). The contract's `qos:` block feeds SIZING
//! (`EntityInventory::from_model`, all four policies since W3).
//!
//! Until this module the two never met. `qos_overrides./t.subscription.depth =
//! 64` was baked into the runtime and **the arena never heard about it** —
//! issue 1190's `BufferTooSmall` with a config-shaped cause and no diagnostic.
//! A node whose parameter said `reliability = reliable` and whose contract said
//! nothing was sized as if best-effort.
//!
//! # Why a bound check is the wrong shape
//!
//! Narrowing is still a disagreement. Honouring
//! `qos_overrides./t.subscription.depth = 1` against a contract's `depth: 64`
//! silently would leave the image running QoS the contract does not describe,
//! and the contract is what every other consumer sizes from — the arena, XRCE's
//! reliable buffers, zenoh's ring depth. The two must say the same thing or the
//! build must stop.
//!
//! Where an override states a policy the contract OMITS, the error names the
//! contract line to add. The contract stays the single authoritative producer;
//! the override must not quietly become a second one.
//!
//! # Scope: capacity, not occupancy
//!
//! Only the four policies [`crate::qos_override::MODELLED_POLICIES`] marks as
//! capacity — `reliability`, `durability`, `history`, `depth`. `deadline`,
//! `lifespan` and the two liveliness policies bound occupancy or liveness of a
//! queue whose size is already decided, the contract has no key for them, and
//! they stay runtime-only.
//!
//! # When this abstains
//!
//! When the model describes NO topic wiring. Then no contract was authored (5
//! of the tree's 114 resolvable models describe wiring, and they are exactly the
//! 5 with a contract sidecar — issue 0973), so there is ONE producer and
//! nothing to disagree with. That abstention is the same one
//! `EntityInventory::from_model` makes by returning `None`, and it is why this
//! wave changes no sizing number in the tree: neither in-tree image that states
//! a `qos_overrides.*` parameter has a contract.

use std::{collections::BTreeMap, fmt};

use ros_launch_manifest_model::SystemModel;

use crate::qos_override::{
    self, QoSOverrideError, SizingStatement, parse_durability, parse_history, parse_reliability,
    role_spelling,
};

/// What the contract says for one endpoint, in the four capacity policies.
///
/// The three enum policies are the RAW strings the contract wrote, not parsed
/// values: an unreadable spelling must show up as a DISAGREEMENT naming what
/// the author typed, never as "the contract is silent" — which is the silent
/// drop the whole W3/W7 pair exists to refuse. (`nros ws entity-inventory`'s
/// `reject_unknown_qos_values` gives that case its own better message on the
/// CLI road; this road must not depend on having been there first.)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ContractQos<'a> {
    reliability: Option<&'a str>,
    durability: Option<&'a str>,
    history: Option<&'a str>,
    depth: Option<u32>,
}

impl<'a> ContractQos<'a> {
    fn of(q: Option<&'a ros_launch_manifest_model::Qos>) -> Self {
        match q {
            None => ContractQos::default(),
            Some(q) => ContractQos {
                reliability: q.reliability.as_deref(),
                durability: q.durability.as_deref(),
                history: q.history.as_deref(),
                depth: q.depth,
            },
        }
    }

    /// The contract's answer for ONE policy: `None` = it states nothing,
    /// `Some(raw)` = what it wrote.
    fn stated(&self, statement: &SizingStatement) -> Option<String> {
        match statement {
            SizingStatement::Reliability(_) => self.reliability.map(str::to_string),
            SizingStatement::Durability(_) => self.durability.map(str::to_string),
            SizingStatement::History(_) => self.history.map(str::to_string),
            SizingStatement::Depth(_) => self.depth.map(|d| d.to_string()),
        }
    }

    /// Does the contract state the SAME value this override states?
    ///
    /// Parsed through the one vocabulary (`parse_reliability` and siblings), so
    /// `best_effort` in a contract and `best_effort` in a parameter are the same
    /// value rather than two strings that happen to match. A contract spelling
    /// that does not parse can equal nothing, which is correct: it does not
    /// state the override's value, and saying so loudly is the point.
    fn agrees_with(&self, statement: &SizingStatement) -> bool {
        match statement {
            SizingStatement::Reliability(p) => {
                self.reliability.and_then(parse_reliability) == Some(*p)
            }
            SizingStatement::Durability(p) => {
                self.durability.and_then(parse_durability) == Some(*p)
            }
            SizingStatement::History(p) => self.history.and_then(parse_history) == Some(*p),
            SizingStatement::Depth(d) => self.depth == Some(*d),
        }
    }
}

/// One `(contract, parameter)` pair that does not state the same fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disagreement {
    /// The node FQN whose parameter states the override.
    pub node: String,
    /// The parameter key, verbatim — one of the two sites.
    pub param_key: String,
    /// What the override states, spelled as the author wrote it.
    pub param_value: String,
    /// The contract file this image resolved, when the model records one.
    pub contract_file: Option<String>,
    /// The contract endpoint ref (`/talker/chatter`), when one exists.
    pub endpoint: Option<String>,
    /// What the contract states for the same policy — `None` when it states
    /// nothing, which is the "params-only policy" case.
    pub contract_value: Option<String>,
    /// The topic, as both sites name it.
    pub topic: String,
    /// `publisher` or `subscription`.
    pub role: &'static str,
    /// The policy in dispute.
    pub policy: &'static str,
}

impl Disagreement {
    /// The `qos:` block a user should paste into the contract, indented as the
    /// schema nests it. Only meaningful when the endpoint ref is known; a
    /// missing endpoint is a bigger gap than one key and says so instead.
    fn contract_line(&self) -> String {
        let Some(ep) = &self.endpoint else {
            let side = if self.role == "publisher" {
                "pub"
            } else {
                "sub"
            };
            return format!(
                "    Declare the endpoint under the node's `{side}:` block, wire it into \
                 `topics:` beneath `{}:`, and give it:\n            qos:\n              {}: {}",
                self.topic, self.policy, self.param_value
            );
        };
        // `/talker/chatter` → node `talker`, endpoint `chatter`. The contract
        // addresses a node by the name its `nodes:` key uses, which is the FQN
        // without its leading `/`.
        let (node, local) = ep
            .rsplit_once('/')
            .map(|(n, l)| (n.trim_start_matches('/'), l))
            .unwrap_or((ep.as_str(), ep.as_str()));
        let side = if self.role == "publisher" {
            "pub"
        } else {
            "sub"
        };
        format!(
            "    nodes:\n      {node}:\n        {side}:\n          {local}:\n            qos:\n\
             \x20             {}: {}",
            self.policy, self.param_value
        )
    }
}

impl fmt::Display for Disagreement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "topic `{}`, {} `{}`:",
            self.topic, self.role, self.policy
        )?;
        writeln!(
            f,
            "  parameter: `{}` = `{}`  (node `{}`)",
            self.param_key, self.param_value, self.node
        )?;
        match (&self.contract_value, &self.endpoint) {
            (Some(v), Some(ep)) => {
                writeln!(
                    f,
                    "  contract : endpoint `{ep}` states `{}: {v}`{}",
                    self.policy,
                    self.contract_file
                        .as_deref()
                        .map(|p| format!("  ({p})"))
                        .unwrap_or_default()
                )?;
                write!(
                    f,
                    "  They are two statements about one fact and they differ. Narrowing is a \
                     disagreement too: the contract is what sizes the arena, the XRCE reliable \
                     buffers and the zenoh ring, so honouring the parameter silently would run \
                     QoS the contract does not describe. Change one of them."
                )
            }
            (Some(v), None) => write!(
                f,
                "  contract : states `{}: {v}` for this topic{}, but declares no {} endpoint on \
                 it for node `{}`.",
                self.policy,
                self.contract_file
                    .as_deref()
                    .map(|p| format!(" ({p})"))
                    .unwrap_or_default(),
                self.role,
                self.node
            ),
            (None, ep) => {
                match ep {
                    Some(ep) => writeln!(
                        f,
                        "  contract : endpoint `{ep}` states no `{}`{}",
                        self.policy,
                        self.contract_file
                            .as_deref()
                            .map(|p| format!("  ({p})"))
                            .unwrap_or_default()
                    )?,
                    None => writeln!(
                        f,
                        "  contract : declares no {} endpoint for node `{}` on this topic{}",
                        self.role,
                        self.node,
                        self.contract_file
                            .as_deref()
                            .map(|p| format!("  ({p})"))
                            .unwrap_or_default()
                    )?,
                }
                writeln!(
                    f,
                    "  The parameter reaches the runtime table and the contract sizes the \
                     buffers, so a policy stated on only one side is an image sized for QoS it \
                     does not run. The contract is the authoritative producer -- state it there \
                     too:"
                )?;
                write!(f, "{}", self.contract_line())
            }
        }
    }
}

/// Every way this check can refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QosAgreementError {
    /// A `qos_overrides.*` parameter this build cannot read at all. The same
    /// refusals [`crate::qos_override::lower`] makes, surfaced here because the
    /// comparison reaches the parameter first and must not quietly skip one.
    Unreadable {
        node: String,
        source: QoSOverrideError,
    },
    /// One or more statements disagree.
    Divergent(Vec<Disagreement>),
}

impl fmt::Display for QosAgreementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QosAgreementError::Unreadable { node, source } => {
                write!(f, "node `{node}`: {source}")
            }
            QosAgreementError::Divergent(ds) => {
                writeln!(
                    f,
                    "{} QoS statement(s) disagree between the launch contract and the \
                     `qos_overrides.*` parameters. They are two statements about one fact \
                     (RFC-0100 D8): the parameters are baked into the runtime QoS table and the \
                     contract is what every size consumer reads, so a divergence is an image \
                     whose buffers do not match its delivery semantics.",
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

impl std::error::Error for QosAgreementError {}

/// The node FQN an endpoint ref belongs to: `/talker/chatter` → `/talker`.
///
/// Same rule as `EntityInventory::from_model`'s `node_of`, and the same reason:
/// the contract addresses an endpoint as `<node fqn>/<local name>`.
fn node_of(ep: &str) -> &str {
    ep.rsplit_once('/').map(|(n, _)| n).unwrap_or(ep)
}

/// Compare every `qos_overrides.*` parameter in `model` against the contract
/// statement for the same endpoint.
///
/// See the module header for the ruling, the scope and the abstention.
pub fn check_model(model: &SystemModel) -> Result<(), QosAgreementError> {
    // No topic wiring means no contract statement about any topic, so there is
    // one producer and nothing to disagree with. NOT a silent pass over a
    // missing check: `EntityInventory::from_model` abstains on exactly this
    // model for exactly this reason, and an image whose sizing has no contract
    // input has no second statement to diverge from.
    if model.structure.topics.is_empty() {
        return Ok(());
    }

    // The contract file each scope resolved from, so a message can name the
    // file to edit rather than "the contract".
    let manifest_of: BTreeMap<&str, &str> = model
        .structure
        .scopes
        .iter()
        .filter_map(|(name, s)| s.manifest.as_deref().map(|m| (name.as_str(), m)))
        .collect();

    let mut out: Vec<Disagreement> = Vec::new();
    for (fqn, inst) in &model.structure.nodes {
        let contract_file = manifest_of.get(inst.scope.as_str()).map(|m| m.to_string());
        for (name, value) in inst.resolved_params(fqn) {
            let value = value.to_bake_string();
            let stated = qos_override::sizing_statement(name.as_str(), &value).map_err(|e| {
                QosAgreementError::Unreadable {
                    node: fqn.clone(),
                    source: e,
                }
            })?;
            let Some(ovr) = stated else { continue };

            let role = role_spelling(ovr.role);
            let endpoints: Vec<&String> = model
                .structure
                .topics
                .get(&ovr.topic)
                .map(|w| {
                    if ovr.role == qos_override::role::PUBLISHER {
                        &w.publishers
                    } else {
                        &w.subscribers
                    }
                })
                .map(|eps| eps.iter().filter(|ep| node_of(ep) == fqn).collect())
                .unwrap_or_default();

            // The contract's topic-level `qos:` block, when there is one, is
            // the fallback statement for a topic whose endpoint states none --
            // it is the SAME fact one level up, so reading it here keeps a
            // contract that states a policy once from reading as silent.
            let topic_qos = ContractQos::of(
                model
                    .contracts
                    .topics
                    .get(&ovr.topic)
                    .and_then(|t| t.qos.as_ref()),
            );

            if endpoints.is_empty() {
                // The contract describes this image's wiring and does not
                // describe this endpoint. The override applies to an entity the
                // contract never priced, which is issue 1190's shape at its
                // widest.
                out.push(Disagreement {
                    node: fqn.clone(),
                    param_key: name.clone(),
                    param_value: ovr.statement.value(),
                    contract_file: contract_file.clone(),
                    endpoint: None,
                    contract_value: topic_qos.stated(&ovr.statement),
                    topic: ovr.topic.clone(),
                    role,
                    policy: ovr.statement.policy(),
                });
                continue;
            }

            for ep in endpoints {
                let q = if ovr.role == qos_override::role::PUBLISHER {
                    ContractQos::of(
                        model
                            .contracts
                            .pub_endpoints
                            .get(ep)
                            .and_then(|c| c.qos.as_ref()),
                    )
                } else {
                    ContractQos::of(
                        model
                            .contracts
                            .sub_endpoints
                            .get(ep)
                            .and_then(|c| c.qos.as_ref()),
                    )
                };
                // An endpoint that states nothing for this policy inherits the
                // topic-level statement, if the contract made one.
                let q = if q.stated(&ovr.statement).is_none() {
                    ContractQos {
                        reliability: q.reliability.or(topic_qos.reliability),
                        durability: q.durability.or(topic_qos.durability),
                        history: q.history.or(topic_qos.history),
                        depth: q.depth.or(topic_qos.depth),
                    }
                } else {
                    q
                };
                if q.agrees_with(&ovr.statement) {
                    continue;
                }
                out.push(Disagreement {
                    node: fqn.clone(),
                    param_key: name.clone(),
                    param_value: ovr.statement.value(),
                    contract_file: contract_file.clone(),
                    endpoint: Some(ep.clone()),
                    contract_value: q.stated(&ovr.statement),
                    topic: ovr.topic.clone(),
                    role,
                    policy: ovr.statement.policy(),
                });
            }
        }
    }

    if out.is_empty() {
        Ok(())
    } else {
        Err(QosAgreementError::Divergent(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_model::{
        NodeInstance, ParamValue, PubContract, Qos, ScopeInfo, SubContract, SystemModel,
        TopicWiring,
    };

    /// A one-topic model: `/talker` publishes `/chatter`, `/listener`
    /// subscribes, and each endpoint carries whatever `qos` the caller hands in.
    fn model(pub_qos: Option<Qos>, sub_qos: Option<Qos>, params: &[(&str, &str)]) -> SystemModel {
        let mut m = SystemModel::default();
        m.structure.scopes.insert(
            "bringup".to_string(),
            ScopeInfo {
                manifest: Some("/ws/launch/bringup.contract.yaml".to_string()),
                ..ScopeInfo::default()
            },
        );
        for (fqn, node_params) in [("/talker", params), ("/listener", &[] as &[(&str, &str)])] {
            m.structure.nodes.insert(
                fqn.to_string(),
                NodeInstance {
                    scope: "bringup".to_string(),
                    pkg: Some("demo".to_string()),
                    params: node_params
                        .iter()
                        .map(|(k, v)| (k.to_string(), ParamValue::Str(v.to_string())))
                        .collect(),
                    ..NodeInstance::default()
                },
            );
        }
        m.structure.topics.insert(
            "/chatter".to_string(),
            TopicWiring {
                msg_type: "std_msgs/msg/Int32".to_string(),
                publishers: vec!["/talker/chatter".to_string()],
                subscribers: vec!["/listener/chatter".to_string()],
            },
        );
        m.contracts.pub_endpoints.insert(
            "/talker/chatter".to_string(),
            PubContract {
                qos: pub_qos,
                ..PubContract::default()
            },
        );
        m.contracts.sub_endpoints.insert(
            "/listener/chatter".to_string(),
            SubContract {
                qos: sub_qos,
                ..SubContract::default()
            },
        );
        m
    }

    fn qos(reliability: Option<&str>, depth: Option<u32>) -> Qos {
        Qos {
            reliability: reliability.map(str::to_string),
            depth,
            ..Qos::default()
        }
    }

    fn divergences(r: Result<(), QosAgreementError>) -> Vec<Disagreement> {
        match r {
            Ok(()) => Vec::new(),
            Err(QosAgreementError::Divergent(d)) => d,
            Err(e) => panic!("expected a divergence, got: {e}"),
        }
    }

    /// ACCEPTANCE 2 -- an agreeing pair builds.
    #[test]
    fn an_agreeing_pair_passes() {
        let m = model(
            Some(qos(Some("best_effort"), Some(8))),
            None,
            &[
                (
                    "qos_overrides./chatter.publisher.reliability",
                    "best_effort",
                ),
                ("qos_overrides./chatter.publisher.depth", "8"),
            ],
        );
        assert_eq!(check_model(&m), Ok(()));
    }

    /// ACCEPTANCE 1 -- a divergent pair fails, naming BOTH sites and BOTH
    /// values.
    #[test]
    fn a_divergent_pair_names_both_sites_and_both_values() {
        let m = model(
            Some(qos(Some("reliable"), Some(8))),
            None,
            &[("qos_overrides./chatter.publisher.depth", "64")],
        );
        let d = divergences(check_model(&m));
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].policy, "depth");
        assert_eq!(d[0].contract_value.as_deref(), Some("8"));
        assert_eq!(d[0].param_value, "64");
        assert_eq!(d[0].endpoint.as_deref(), Some("/talker/chatter"));
        let text = QosAgreementError::Divergent(d).to_string();
        for want in [
            "qos_overrides./chatter.publisher.depth",
            "/ws/launch/bringup.contract.yaml",
            "/talker/chatter",
            "`depth: 8`",
            "`64`",
        ] {
            assert!(text.contains(want), "missing `{want}` in:\n{text}");
        }
    }

    /// NARROWING is a disagreement too -- the ruling is not a bound check.
    #[test]
    fn narrowing_is_a_disagreement() {
        let m = model(
            Some(qos(None, Some(64))),
            None,
            &[("qos_overrides./chatter.publisher.depth", "1")],
        );
        let d = divergences(check_model(&m));
        assert_eq!(d.len(), 1, "a narrowing override must not pass: {d:#?}");
        assert_eq!(d[0].contract_value.as_deref(), Some("64"));
    }

    /// ACCEPTANCE 3 -- a params-only policy fails, naming the contract line to
    /// add.
    #[test]
    fn a_params_only_policy_names_the_contract_line_to_add() {
        let m = model(
            Some(qos(None, Some(8))),
            None,
            &[(
                "qos_overrides./chatter.publisher.reliability",
                "best_effort",
            )],
        );
        let d = divergences(check_model(&m));
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].contract_value, None);
        let text = QosAgreementError::Divergent(d).to_string();
        for want in [
            "states no `reliability`",
            "nodes:",
            "talker:",
            "pub:",
            "chatter:",
            "qos:",
            "reliability: best_effort",
        ] {
            assert!(text.contains(want), "missing `{want}` in:\n{text}");
        }
    }

    /// ACCEPTANCE 4 -- a contract-only policy builds. That is the normal case
    /// and every in-tree contract is it.
    #[test]
    fn a_contract_only_policy_passes() {
        let m = model(
            Some(qos(Some("reliable"), Some(8))),
            Some(qos(Some("best_effort"), Some(3))),
            &[],
        );
        assert_eq!(check_model(&m), Ok(()));
    }

    /// A model with no topic wiring abstains -- nobody authored a contract, so
    /// there is one producer. This is what keeps every existing image building
    /// and every sizing number where it was.
    #[test]
    fn no_contract_means_nothing_to_disagree_with() {
        let mut m = model(
            None,
            None,
            &[(
                "qos_overrides./chatter.publisher.reliability",
                "best_effort",
            )],
        );
        m.structure.topics.clear();
        m.contracts.pub_endpoints.clear();
        m.contracts.sub_endpoints.clear();
        assert_eq!(check_model(&m), Ok(()));
    }

    /// The role is part of the pairing: an override on the SUBSCRIPTION side
    /// must not be compared against the publisher's contract.
    #[test]
    fn the_role_selects_which_endpoint_is_compared() {
        // The publisher states depth 8; the subscriber states 3. An override
        // for the subscription role at 8 therefore DISAGREES, and the
        // disagreement must name the subscriber's endpoint.
        let mut m = model(
            Some(qos(None, Some(8))),
            Some(qos(None, Some(3))),
            &[("qos_overrides./chatter.subscription.depth", "8")],
        );
        // Move the parameter onto the node that actually subscribes.
        let p = m.structure.nodes["/talker"].params.clone();
        m.structure.nodes.get_mut("/talker").unwrap().params.clear();
        m.structure.nodes.get_mut("/listener").unwrap().params = p;
        let d = divergences(check_model(&m));
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].endpoint.as_deref(), Some("/listener/chatter"));
        assert_eq!(d[0].contract_value.as_deref(), Some("3"));
        assert_eq!(d[0].role, "subscription");
    }

    /// An override whose node has no such endpoint in the contract is the
    /// widest form of "the contract is silent": the override applies to an
    /// entity nothing priced.
    #[test]
    fn an_override_on_an_undeclared_endpoint_is_a_disagreement() {
        let m = model(
            Some(qos(Some("reliable"), Some(8))),
            None,
            &[("qos_overrides./chatter.subscription.depth", "4")],
        );
        // `/talker` publishes `/chatter`; it subscribes to nothing.
        let d = divergences(check_model(&m));
        assert_eq!(d.len(), 1, "{d:#?}");
        assert_eq!(d[0].endpoint, None);
        let text = QosAgreementError::Divergent(d).to_string();
        assert!(text.contains("declares no subscription endpoint"), "{text}");
        // The remedy is prose here rather than a pasteable block, and it must
        // still read as YAML -- an escaped `\n` in a format string would print
        // the two characters and tell the user to type them.
        assert!(
            !text.contains("\\n"),
            "the remedy must not print `\\n`:\n{text}"
        );
        assert!(
            text.contains("\n            qos:\n              depth: 4"),
            "{text}"
        );
    }

    /// The topic-level `qos:` block is the SAME fact one level up, so a
    /// contract that states a policy once does not read as silent.
    #[test]
    fn a_topic_level_contract_statement_counts() {
        let mut m = model(
            None,
            None,
            &[(
                "qos_overrides./chatter.publisher.reliability",
                "best_effort",
            )],
        );
        m.contracts.topics.insert(
            "/chatter".to_string(),
            ros_launch_manifest_model::TopicContract {
                qos: Some(qos(Some("best_effort"), None)),
                ..ros_launch_manifest_model::TopicContract::default()
            },
        );
        assert_eq!(check_model(&m), Ok(()));
        // …and it diverges when it says something else.
        m.contracts.topics.get_mut("/chatter").unwrap().qos = Some(qos(Some("reliable"), None));
        assert_eq!(divergences(check_model(&m)).len(), 1);
    }

    /// The four capacity policies, all of them -- this comparison is not the
    /// depth check with three siblings left out.
    #[test]
    fn all_four_capacity_policies_are_compared() {
        for (policy, contract_value, override_value) in [
            ("reliability", "reliable", "best_effort"),
            ("durability", "volatile", "transient_local"),
            ("history", "keep_last", "keep_all"),
            ("depth", "8", "64"),
        ] {
            let mut q = Qos::default();
            match policy {
                "reliability" => q.reliability = Some(contract_value.to_string()),
                "durability" => q.durability = Some(contract_value.to_string()),
                "history" => q.history = Some(contract_value.to_string()),
                _ => q.depth = Some(contract_value.parse().unwrap()),
            }
            let key = format!("qos_overrides./chatter.publisher.{policy}");
            let m = model(Some(q), None, &[(key.as_str(), override_value)]);
            let d = divergences(check_model(&m));
            assert_eq!(d.len(), 1, "{policy} must be compared: {d:#?}");
            assert_eq!(d[0].policy, policy);
            assert_eq!(d[0].contract_value.as_deref(), Some(contract_value));
            assert_eq!(d[0].param_value, override_value);
        }
    }

    /// The three occupancy policies are OUT of scope and must stay silent --
    /// the contract has no key for them, so comparing them would refuse every
    /// legal `deadline` override in the tree.
    #[test]
    fn occupancy_policies_are_not_compared() {
        for (policy, value) in [
            ("deadline", "100"),
            ("lifespan", "250"),
            ("liveliness", "automatic"),
            ("liveliness_lease_duration", "500"),
        ] {
            let key = format!("qos_overrides./chatter.publisher.{policy}");
            let m = model(Some(qos(None, Some(8))), None, &[(key.as_str(), value)]);
            assert_eq!(check_model(&m), Ok(()), "{policy} must not be compared");
        }
    }

    /// An override this build cannot read is an ERROR here too. The comparison
    /// reaches the parameter before the bake does, and a skip would be the
    /// silence issue 0303 removed.
    #[test]
    fn an_unreadable_override_is_an_error_not_a_skip() {
        let m = model(
            Some(qos(None, Some(8))),
            None,
            &[("qos_overrides./chatter.pub.depth", "8")],
        );
        let e = check_model(&m).unwrap_err();
        assert!(matches!(e, QosAgreementError::Unreadable { .. }), "{e:?}");
        assert!(e.to_string().contains("publisher"), "{e}");
    }
}
