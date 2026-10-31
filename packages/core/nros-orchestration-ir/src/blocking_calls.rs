//! phase-509 P8 (RFC-0108 D8.3) -- refuse a blocking call to a level that
//! cannot preempt the client.
//!
//! A callback that sends a request and waits for the response holds its
//! worker until the server has run. The server's callback can run only on a
//! worker that may preempt the client's:
//!
//! - **The client's own level:** the server waits for the worker the client
//!   holds. A deadlock.
//! - **A less urgent level** (or a different level at the same kernel
//!   priority, which preempts nothing): the client waits for work it would
//!   otherwise preempt, and every level in between delays the server. A
//!   priority inversion bounded by all of that work.
//! - **A more urgent level:** the server preempts the client and answers.
//!   Allowed.
//!
//! The contract says how a client waits (rlm v0.1.69): `cli.<e>.call:
//! blocking | async`, and `actions.<a>.call` for each client the action lists
//! (`Contracts::cli_endpoints`, `Contracts::action_cli_endpoints`).
//!
//! - **`blocking`** to an equal or less urgent level is refused by name.
//! - **`async`** (send now, take the response on a later release) is never
//!   checked: the sender never waits.
//! - **Unstated** is not `async`. A call with no `call:` to an equal or less
//!   urgent level is REPORTED, never refused, so a contract written before
//!   the key keeps building and says what it would take to check it.
//!
//! **What a level is here.** Until the level runners land (phase-509 P9) a
//! level is a tier of the image's resolved tier table: one thread at the
//! tier's priority. A tierless image (no table, or the one synthesized
//! `default` tier) is one level. A node of a multi-tier image that no tier
//! holds has an unknown level: a call it makes or serves is reported, not
//! refused. A server outside the image (no level here at all) is a call
//! across images, which no worker of this one holds: not checked.
//!
//! Every tier member is a bare node name (the component instance name the
//! three bake roads key their tables by); a contract endpoint ref
//! `"<node FQN>/<endpoint>"` is matched by the last segment of its node FQN.

use std::collections::{BTreeMap, BTreeSet};

use ros_launch_manifest_model::{CallMode, CliContract, ServiceWiring, SystemModel};

use crate::ResolvedTierTable;

/// One level a node's callbacks run on.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LevelRef {
    /// The tier's name (`default` for a tierless image).
    pub name: String,
    /// The tier's kernel priority, in the RTOS's own direction.
    pub priority: i64,
}

/// Where each node of one image runs: the levels of its callback groups.
#[derive(Clone, Debug, Default)]
pub struct ImageLevels {
    by_node: BTreeMap<String, BTreeSet<LevelRef>>,
    /// Image members whose level is not known (a multi-tier image that
    /// places none of their groups).
    unplaced: BTreeSet<String>,
    low_number_is_high: bool,
}

impl ImageLevels {
    /// A tierless image: every member on one level.
    pub fn one_level<'a>(members: impl IntoIterator<Item = &'a str>) -> Self {
        let level = LevelRef {
            name: crate::DEFAULT_TIER.to_string(),
            priority: 0,
        };
        Self {
            by_node: members
                .into_iter()
                .map(|m| (m.to_string(), BTreeSet::from([level.clone()])))
                .collect(),
            unplaced: BTreeSet::new(),
            low_number_is_high: false,
        }
    }

    /// From the image's resolved tier table: until P9 each tier is a level.
    /// `members` is every node of the image (bare names); one the table does
    /// not hold is unplaced, unless the table has one tier, which then holds
    /// the whole image.
    pub fn from_tiers<'a>(
        table: &ResolvedTierTable,
        low_number_is_high: bool,
        members: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        if table.tiers.len() <= 1 {
            let mut one = Self::one_level(members);
            if let Some(t) = table.tiers.first() {
                let level = LevelRef {
                    name: t.name.clone(),
                    priority: t.priority,
                };
                for levels in one.by_node.values_mut() {
                    *levels = BTreeSet::from([level.clone()]);
                }
                for (node, _) in &t.members {
                    one.by_node
                        .insert(node.clone(), BTreeSet::from([level.clone()]));
                }
            }
            return one;
        }
        let mut by_node: BTreeMap<String, BTreeSet<LevelRef>> = BTreeMap::new();
        for t in &table.tiers {
            for (node, _group) in &t.members {
                by_node.entry(node.clone()).or_default().insert(LevelRef {
                    name: t.name.clone(),
                    priority: t.priority,
                });
            }
        }
        let unplaced = members
            .into_iter()
            .filter(|m| !by_node.contains_key(*m))
            .map(str::to_string)
            .collect();
        Self {
            by_node,
            unplaced,
            low_number_is_high,
        }
    }

    /// True when `a` preempts `b`: a different level at a strictly more
    /// urgent kernel priority.
    fn preempts(&self, a: &LevelRef, b: &LevelRef) -> bool {
        a.name != b.name
            && if self.low_number_is_high {
                a.priority < b.priority
            } else {
                a.priority > b.priority
            }
    }
}

/// How a client's server stands against it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Relation {
    /// The server runs on the client's own level: a blocking call deadlocks.
    SameLevel { level: String },
    /// The server's level cannot preempt the client's: a blocking call is a
    /// priority inversion.
    NotMoreUrgent {
        client_level: String,
        client_priority: i64,
        server_level: String,
        server_priority: i64,
    },
    /// One side's level is not known (a node no tier of a multi-tier image
    /// holds).
    Unplaced { node: String },
}

/// One client/server pair the check has something to say about.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CallFinding {
    /// `"service"` or `"action"`.
    pub kind: &'static str,
    /// The client endpoint ref.
    pub client: String,
    /// The server endpoint ref.
    pub server: String,
    /// The client's stated call mode; `None` when unstated.
    pub call: Option<CallModeName>,
    pub relation: Relation,
}

/// [`CallMode`] with an order, for [`CallFinding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallModeName {
    Blocking,
}

impl CallFinding {
    /// One line naming the pair, where each side runs, and what fixes it.
    pub fn render(&self) -> String {
        let what = match &self.relation {
            Relation::SameLevel { level } => format!(
                "both run on level `{level}`, so a blocking call deadlocks (the server \
                 cannot run while the client holds the level's worker)"
            ),
            Relation::NotMoreUrgent {
                client_level,
                client_priority,
                server_level,
                server_priority,
            } => format!(
                "the server's level `{server_level}` (priority {server_priority}) cannot \
                 preempt the client's `{client_level}` (priority {client_priority}), so a \
                 blocking call is a priority inversion bounded by all the work in between"
            ),
            Relation::Unplaced { node } => format!(
                "node `{node}` is on no level of this image's tier table, so the call \
                 cannot be checked"
            ),
        };
        match (&self.call, &self.relation) {
            (Some(CallModeName::Blocking), _) => format!(
                "{} client `{}` calls `{}` with `call: blocking`: {what}. Make the call \
                 `async` (send now, take the response on a later release), or run the \
                 server at a more urgent level",
                self.kind, self.client, self.server
            ),
            (None, Relation::Unplaced { .. }) => format!(
                "{} client `{}` calls `{}`: {what}",
                self.kind, self.client, self.server
            ),
            (None, _) => format!(
                "{} client `{}` calls `{}` and states no `call:`: {what} if it waits. State \
                 `call: async` or `call: blocking` on the client so the build can check it",
                self.kind, self.client, self.server
            ),
        }
    }
}

/// The refusal [`check_model`] returns: every blocking call whose server
/// cannot preempt the client.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{}", render_refusal(refused))]
pub struct BlockingCallRefused {
    pub refused: Vec<CallFinding>,
}

fn render_refusal(refused: &[CallFinding]) -> String {
    let mut s = format!(
        "RFC-0108 D8.3: {} blocking call(s) wait for a server at a level that cannot \
         preempt the client:",
        refused.len()
    );
    for f in refused {
        s.push_str("\n  - ");
        s.push_str(&f.render());
    }
    s
}

/// The node part of an endpoint ref, as a bare name.
fn bare_node(endpoint: &str) -> &str {
    let node = endpoint.rsplit_once('/').map_or(endpoint, |(n, _)| n);
    node.rsplit('/').next().unwrap_or(node)
}

fn findings_for(
    kind: &'static str,
    wiring: &ServiceWiring,
    calls: &BTreeMap<String, CliContract>,
    levels: &ImageLevels,
    out: &mut BTreeSet<CallFinding>,
) {
    for client in &wiring.client {
        let call = match calls.get(client).and_then(|c| c.call) {
            Some(CallMode::Async) => continue,
            Some(CallMode::Blocking) => Some(CallModeName::Blocking),
            None => None,
        };
        let cnode = bare_node(client);
        for server in &wiring.server {
            let snode = bare_node(server);
            let push = |relation: Relation, out: &mut BTreeSet<CallFinding>| {
                out.insert(CallFinding {
                    kind,
                    client: client.clone(),
                    server: server.clone(),
                    call,
                    relation,
                });
            };
            for node in [cnode, snode] {
                if levels.unplaced.contains(node) {
                    push(
                        Relation::Unplaced {
                            node: node.to_string(),
                        },
                        out,
                    );
                }
            }
            let (Some(cl), Some(sl)) = (levels.by_node.get(cnode), levels.by_node.get(snode))
            else {
                continue;
            };
            for c in cl {
                for s in sl {
                    if c.name == s.name {
                        push(
                            Relation::SameLevel {
                                level: c.name.clone(),
                            },
                            out,
                        );
                    } else if !levels.preempts(s, c) {
                        push(
                            Relation::NotMoreUrgent {
                                client_level: c.name.clone(),
                                client_priority: c.priority,
                                server_level: s.name.clone(),
                                server_priority: s.priority,
                            },
                            out,
                        );
                    }
                }
            }
        }
    }
}

/// RFC-0108 D8.3 over one image. `Err` names every `call: blocking` whose
/// server runs at the client's own level or one that cannot preempt it.
/// `Ok` carries the reports: each client with no stated call to such a
/// server, and each call a node with no known level makes or serves.
pub fn check_model(
    model: &SystemModel,
    levels: &ImageLevels,
) -> Result<Vec<CallFinding>, BlockingCallRefused> {
    let mut found = BTreeSet::new();
    for w in model.structure.services.values() {
        findings_for(
            "service",
            w,
            &model.contracts.cli_endpoints,
            levels,
            &mut found,
        );
    }
    for w in model.structure.actions.values() {
        findings_for(
            "action",
            w,
            &model.contracts.action_cli_endpoints,
            levels,
            &mut found,
        );
    }
    let (refused, reports): (Vec<_>, Vec<_>) = found.into_iter().partition(|f| {
        f.call == Some(CallModeName::Blocking) && !matches!(f.relation, Relation::Unplaced { .. })
    });
    if refused.is_empty() {
        Ok(reports)
    } else {
        Err(BlockingCallRefused { refused })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolvedTier;

    fn tier(name: &str, priority: i64, members: &[&str]) -> ResolvedTier {
        ResolvedTier {
            name: name.to_string(),
            priority,
            stack_bytes: None,
            spin_period_us: None,
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: members
                .iter()
                .map(|m| (m.to_string(), "g".to_string()))
                .collect(),
        }
    }

    /// Zephyr-direction table (lower number more urgent): `hi` at 5 holds
    /// `handler` and `peer`; `lo` at 8 holds `operator`.
    fn levels() -> ImageLevels {
        let table = ResolvedTierTable {
            tiers: vec![
                tier("lo", 8, &["operator"]),
                tier("hi", 5, &["handler", "peer"]),
            ],
        };
        ImageLevels::from_tiers(&table, true, ["handler", "peer", "operator"])
    }

    /// `handler` calls a server on `operator` (less urgent) and one on
    /// `peer` (its own level); `operator` calls one on `handler` (more
    /// urgent). Each client's call mode is `call`.
    fn model(call: Option<CallMode>) -> SystemModel {
        let mut m = SystemModel::default();
        for (svc, server, client) in [
            ("/down", "/s/operator/down", "/s/handler/down"),
            ("/same", "/s/peer/same", "/s/handler/same"),
            ("/up", "/s/handler/up", "/s/operator/up"),
        ] {
            m.structure.services.insert(
                svc.to_string(),
                ServiceWiring {
                    srv_type: "x/srv/T".to_string(),
                    server: vec![server.to_string()],
                    client: vec![client.to_string()],
                },
            );
            if call.is_some() {
                m.contracts.cli_endpoints.insert(
                    client.to_string(),
                    CliContract {
                        call,
                        ..Default::default()
                    },
                );
            }
        }
        m
    }

    /// A blocking call to a less urgent level and one to the client's own
    /// level are refused, each by name; the call up to a more urgent level
    /// passes.
    #[test]
    fn a_blocking_call_to_an_equal_or_less_urgent_level_is_refused() {
        let err = check_model(&model(Some(CallMode::Blocking)), &levels()).unwrap_err();
        assert_eq!(err.refused.len(), 2, "{err}");
        let msg = err.to_string();
        assert!(
            msg.contains("`/s/handler/down` calls `/s/operator/down`"),
            "{msg}"
        );
        assert!(msg.contains("priority inversion"), "{msg}");
        assert!(
            msg.contains("`/s/handler/same` calls `/s/peer/same`"),
            "{msg}"
        );
        assert!(msg.contains("deadlocks"), "{msg}");
        assert!(!msg.contains("/s/operator/up"), "{msg}");
    }

    /// The same pairs with `call: async` pass, and report nothing.
    #[test]
    fn an_async_call_passes() {
        assert_eq!(
            check_model(&model(Some(CallMode::Async)), &levels()),
            Ok(vec![])
        );
    }

    /// With no `call:` stated the pairs are reported, never refused.
    #[test]
    fn an_unstated_call_is_reported_not_refused() {
        let reports = check_model(&model(None), &levels()).expect("unstated never refuses");
        assert_eq!(
            reports
                .iter()
                .map(|f| f.client.as_str())
                .collect::<Vec<_>>(),
            ["/s/handler/down", "/s/handler/same"]
        );
        assert!(reports[0].render().contains("states no `call:`"));
    }

    /// A tierless image is one level: any blocking call inside it deadlocks,
    /// and a server outside the image is not checked.
    #[test]
    fn a_tierless_image_is_one_level() {
        let one = ImageLevels::one_level(["handler", "peer", "operator"]);
        let err = check_model(&model(Some(CallMode::Blocking)), &one).unwrap_err();
        assert_eq!(err.refused.len(), 3);
        assert!(
            err.refused
                .iter()
                .all(|f| matches!(f.relation, Relation::SameLevel { .. }))
        );
        let outside = ImageLevels::one_level(["handler", "peer"]);
        let err = check_model(&model(Some(CallMode::Blocking)), &outside).unwrap_err();
        assert_eq!(err.refused.len(), 1, "only handler -> peer is in the image");
    }

    /// The direction follows the kernel: on a bigger-number-wins kernel the
    /// same priorities invert which call is refused. Two tiers at one
    /// priority preempt nothing, so that call is refused too.
    #[test]
    fn urgency_follows_the_kernels_direction() {
        let table = ResolvedTierTable {
            tiers: vec![
                tier("lo", 8, &["operator"]),
                tier("hi", 5, &["handler", "peer"]),
            ],
        };
        let posix = ImageLevels::from_tiers(&table, false, ["handler", "peer", "operator"]);
        let err = check_model(&model(Some(CallMode::Blocking)), &posix).unwrap_err();
        let clients: Vec<_> = err.refused.iter().map(|f| f.client.as_str()).collect();
        assert_eq!(clients, ["/s/handler/same", "/s/operator/up"]);
        let flat = ResolvedTierTable {
            tiers: vec![
                tier("a", 5, &["operator"]),
                tier("b", 5, &["handler", "peer"]),
            ],
        };
        let flat = ImageLevels::from_tiers(&flat, true, ["handler", "peer", "operator"]);
        assert_eq!(
            check_model(&model(Some(CallMode::Blocking)), &flat)
                .unwrap_err()
                .refused
                .len(),
            3
        );
    }

    /// An action client is checked the same way, from
    /// `action_cli_endpoints`; a node no tier holds is reported.
    #[test]
    fn an_action_client_is_checked_and_an_unplaced_node_reported() {
        let mut m = SystemModel::default();
        m.structure.actions.insert(
            "/go".to_string(),
            ServiceWiring {
                srv_type: "x/action/Go".to_string(),
                server: vec!["/s/operator/go".to_string()],
                client: vec!["/s/handler/go".to_string(), "/s/stray/go".to_string()],
            },
        );
        m.contracts.action_cli_endpoints.insert(
            "/s/handler/go".to_string(),
            CliContract {
                call: Some(CallMode::Blocking),
                ..Default::default()
            },
        );
        let table = ResolvedTierTable {
            tiers: vec![tier("lo", 8, &["operator"]), tier("hi", 5, &["handler"])],
        };
        let lv = ImageLevels::from_tiers(&table, true, ["handler", "operator", "stray"]);
        let err = check_model(&m, &lv).unwrap_err();
        assert_eq!(err.refused.len(), 1);
        assert!(err.to_string().contains("action client `/s/handler/go`"));
        m.contracts.action_cli_endpoints.clear();
        let reports = check_model(&m, &lv).unwrap();
        assert!(
            reports
                .iter()
                .any(|f| matches!(&f.relation, Relation::Unplaced { node } if node == "stray")),
            "{reports:?}"
        );
    }
}
