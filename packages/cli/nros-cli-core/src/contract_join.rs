//! phase-454 W12 (RFC-0100 D3) — joining the CONTRACT's QoS onto the rows the
//! metadata probe found.
//!
//! # The gap this closes
//!
//! RFC-0100 D3 says the contract file is the single declaration surface, and
//! phase-454 W11 measured that the gap was open in both directions at once:
//!
//! > *"the road that carries contracts (a workspace bringup) writes no
//! > descriptor, and the road that writes descriptors reads no contract."*
//!
//! `EntityInventory::from_model` has resolved all four QoS policies out of a
//! `*.contract.yaml` since W3, and a synced leaf has had a resolved SystemModel
//! beside its probe output for as long. Nothing joined them, so the descriptor
//! `nros sync` writes carried the PROBE's rows: the `topic` column held the
//! callback name, no row stated a `depth`, and `undeclared_endpoints` came out
//! non-zero — which is the guard that switches every per-endpoint consumer off
//! (D6: absence is not zero).
//!
//! # THE KEY, and why it is safe
//!
//! The two inventories key their rows differently and neither key is the
//! other's:
//!
//! | side | row identity |
//! | --- | --- |
//! | probe | `(kind, type, id)` — and `id` is the CALLBACK name for a subscription |
//! | contract | `(kind, type, RESOLVED topic)` |
//!
//! Joining on `id` would attribute `/chatter`'s declared depth to whatever row
//! happened to sort first, and a depth published against the wrong endpoint is
//! an **UNDER-size** — the direction that ships `NodeError::BufferTooSmall`
//! rather than wasted bytes. So the join needs a key both sides can state, and
//! there is exactly one: **the topic**.
//!
//! The probe records it. [`EntityDecl::source_topic`] carries the
//! `unresolved_topic` / `unresolved_name` the harness writes — the name AS THE
//! SOURCE SPELLS IT. That is not automatically the name the contract states,
//! because a contract states the RESOLVED name, and resolution applies two
//! things this reader must not guess at:
//!
//! * **the node's namespace**, which turns `chatter` into `/ns/chatter`;
//! * **the launch remappings**, which can turn any name into any other.
//!
//! So the join is certain in exactly one case and refuses in every other:
//!
//! | probe row | what happens |
//! | --- | --- |
//! | the node declares NO remaps, the written name is ABSOLUTE, and `(kind, type, name)` picks out ONE row on each side | attributed |
//! | the node declares any remap | REFUSED — a remap can rename this endpoint and the model does not say which endpoint it applied to |
//! | the written name is relative (`chatter`) or private (`~/chatter`) | REFUSED — resolving it needs the namespace, and the namespace is only half the answer without the remaps |
//! | the key picks out zero, or more than one, on EITHER side | REFUSED, naming the row and what the contract does describe |
//!
//! **Both sides, not just the contract's.** Two registrations on one topic and
//! one contract endpoint is an under-description: attributing the declaration
//! to whichever registration the probe listed first and refusing the other
//! publishes a depth for a registration nobody declared. The count has to be
//! one on each side or neither row is attributed.
//!
//! A refusal is per ROW and per FACT (D6). It never widens the basis, never
//! floors anything, and never degrades another row: the refused row's four QoS
//! policies become `Fact::Refused` with the reason, it keeps counting toward
//! `undeclared_endpoints`, and every consumer that needs per-endpoint
//! attribution therefore keeps its worst case — which is the whole point of
//! that guard.
//!
//! # And when no contract was authored at all
//!
//! Nothing here runs. `EntityInventory::from_model` returns `None` when the
//! model describes no wiring, and this module then hands the probe inventory
//! back untouched — so a leaf with no contract produces the same rows, the same
//! descriptor and the same image it did before this wave. "Nobody said"
//! (`Fact::Absent`) and "I looked and could not tell" (`Fact::Refused`) are
//! different statements and only the second one is this module's to make.

use std::collections::{BTreeMap, BTreeSet};

use crate::entity_inventory::{
    ComponentEntities, Declaration, EntityDecl, EntityInventory, EntityKind,
};

/// What [`join`] produced.
pub struct Joined {
    /// The probe's rows, with contract QoS attributed where it could be.
    pub inventory: EntityInventory,
    /// Did the model describe any wiring at all — i.e. was a contract
    /// authored for this image?
    ///
    /// `false` means this join did nothing, which is a different state from
    /// "a contract exists and matched nothing" and the two must not print the
    /// same line.
    pub contract_seen: bool,
    /// One line per row that could not be attributed, and per contract
    /// endpoint that matched no row. For the caller to PRINT: a refusal the
    /// user cannot read is a refusal they work around.
    pub notes: Vec<String>,
}

/// The join key: the kind, the ROS type, and the RESOLVED endpoint name.
type Key = (EntityKind, String, String);

/// Join a contract's per-endpoint QoS onto a probe-derived inventory.
///
/// See the module docs for the key and for what refuses. `probe` is returned
/// unchanged when the model describes no wiring.
#[must_use]
pub fn join(probe: &EntityInventory, model: &ros_launch_manifest_model::SystemModel) -> Joined {
    let Some(contract) = EntityInventory::from_model("contract", model) else {
        return Joined {
            inventory: probe.clone(),
            contract_seen: false,
            notes: Vec::new(),
        };
    };

    // Which components the model applies name REMAPPINGS to. Keyed by the last
    // segment of the node FQN, which is the same string `from_model` uses as a
    // component name and the same one `nano_ros_node_register(NAME ...)` gives
    // (RFC-0057 requires them equal).
    let remapped: BTreeSet<String> = model
        .structure
        .nodes
        .iter()
        .filter(|(_, n)| !n.remaps.is_empty())
        .map(|(fqn, _)| component_of(fqn))
        .collect();

    // The contract's rows, per component, indexed by the join key. `Vec` and
    // not a single value because a duplicate key is the ambiguity this join
    // refuses on, and a map that silently kept the last one would be the
    // mis-attribution wearing a lookup's clothes.
    let mut declared: BTreeMap<String, BTreeMap<Key, Vec<EntityDecl>>> = BTreeMap::new();
    for c in contract.components() {
        let per_component = declared.entry(c.component.clone()).or_default();
        for e in c.declaration.entities() {
            if let Some(k) = key_of(e, e.name.as_deref()) {
                per_component.entry(k).or_default().push(e.clone());
            }
        }
    }

    let mut notes: Vec<String> = Vec::new();
    let mut claimed: BTreeSet<(String, Key)> = BTreeSet::new();
    let mut rows_out: Vec<ComponentEntities> = Vec::new();

    for c in probe.components() {
        let Declaration::Stated(rows) = &c.declaration else {
            rows_out.push((*c).clone());
            continue;
        };
        // How many registrations THIS image makes under each key. The second
        // half of the uniqueness rule.
        let mut registrations: BTreeMap<Key, usize> = BTreeMap::new();
        for r in rows {
            if let Some(k) = key_of(r, r.source_topic.as_deref()) {
                *registrations.entry(k).or_default() += 1;
            }
        }
        let empty = BTreeMap::new();
        let per_component = declared.get(&c.component).unwrap_or(&empty);
        let joined: Vec<EntityDecl> = rows
            .iter()
            .map(|row| {
                attribute(
                    row,
                    &c.component,
                    &remapped,
                    per_component,
                    &registrations,
                    &mut claimed,
                    &mut notes,
                )
            })
            .collect();
        rows_out.push(ComponentEntities {
            declaration: Declaration::Stated(joined),
            ..(*c).clone()
        });
    }

    // Contract endpoints nothing claimed. Not a refusal — every row that IS
    // priced was priced from a key unique on both sides, so a leftover cannot
    // have made one of those wrong. It is still a disagreement between what the
    // contract describes and what the code creates, and saying so is how a
    // contract stops rotting.
    for (component, per_component) in &declared {
        for (k, eps) in per_component {
            if claimed.contains(&(component.clone(), k.clone())) {
                continue;
            }
            for e in eps {
                notes.push(format!(
                    "the contract declares a {} of `{}` on `{}` for component `{component}`, and \
                     the code creates no such endpoint -- its declaration prices nothing",
                    e.kind.tag(),
                    e.type_name.as_deref().unwrap_or("<untyped>"),
                    e.name.as_deref().unwrap_or("<unnamed>"),
                ));
            }
        }
    }

    Joined {
        inventory: probe.with_components(rows_out),
        contract_seen: true,
        notes,
    }
}

/// `/ns/listener` -> `listener`; the component name both inventories use.
fn component_of(node_fqn: &str) -> String {
    node_fqn.rsplit('/').next().unwrap_or(node_fqn).to_string()
}

/// The join key for a row, or `None` when the row cannot carry one.
///
/// `None` for a kind with no endpoint identity (a timer subscribes to nothing),
/// for a row with no type, and for a name that is not ABSOLUTE — the last
/// because a contract states resolved names and a relative or private spelling
/// is not one.
fn key_of(row: &EntityDecl, name: Option<&str>) -> Option<Key> {
    if !is_endpoint(row.kind) {
        return None;
    }
    let ty = row.type_name.as_deref()?;
    let name = name?;
    if !name.starts_with('/') {
        return None;
    }
    Some((row.kind, ty.to_string(), name.to_string()))
}

/// Does this kind name an endpoint a contract can describe?
///
/// `EntityKind::carries_qos_depth` draws the same line for the depth table; a
/// timer and a guard condition carry no type and no name, so they have no
/// identity this join could key on and no QoS to gain.
fn is_endpoint(kind: EntityKind) -> bool {
    !matches!(kind, EntityKind::Timer | EntityKind::GuardCondition)
}

/// Attribute ONE probe row to a contract endpoint, or refuse it by name.
fn attribute(
    row: &EntityDecl,
    component: &str,
    remapped: &BTreeSet<String>,
    declared: &BTreeMap<Key, Vec<EntityDecl>>,
    registrations: &BTreeMap<Key, usize>,
    claimed: &mut BTreeSet<(String, Key)>,
    notes: &mut Vec<String>,
) -> EntityDecl {
    if !is_endpoint(row.kind) {
        return row.clone();
    }

    let of_kind: Vec<&EntityDecl> = declared
        .iter()
        .filter(|((k, _, _), _)| *k == row.kind)
        .flat_map(|(_, v)| v.iter())
        .collect();
    if of_kind.is_empty() {
        // The contract describes no endpoint of this kind for this component.
        // Still a REFUSAL and not an absence: a contract WAS authored for this
        // image, so this reader looked and the answer is "it does not say".
        return refused(
            row,
            format!(
                "the contract for this image describes no {} for component `{component}`, so \
                 there is no endpoint to attribute this one to. Declare it under \
                 `nodes.{component}.{}` in the contract sidecar and wire it in `topics:` / \
                 `services:` / `actions:`",
                row.kind.tag(),
                contract_block(row.kind),
            ),
            notes,
        );
    }

    if remapped.contains(component) {
        return refused(
            row,
            format!(
                "the launch description applies name REMAPPINGS to node `{component}`, so the \
                 name this endpoint registers under is not the name its source writes, and the \
                 model does not say which endpoint each remap renamed. Attributing a declared \
                 depth to the wrong endpoint is an UNDER-size, so it is refused rather than \
                 guessed (RFC-0100 D6)"
            ),
            notes,
        );
    }

    let Some(source) = row.source_topic.as_deref() else {
        return refused(
            row,
            format!(
                "this {} carries no written topic in the leaf's `metadata/` probe, so there is \
                 nothing to compare with the resolved name the contract states",
                row.kind.tag()
            ),
            notes,
        );
    };

    let Some(key) = key_of(row, Some(source)) else {
        return refused(
            row,
            format!(
                "this {} registers on `{source}`, which is not an absolute name. A contract \
                 states RESOLVED names, and resolving `{source}` needs the node's namespace AND \
                 its remappings -- neither of which reaches this reader per endpoint. Write the \
                 name absolutely at the call site, or declare the endpoint the contract already \
                 names",
                row.kind.tag()
            ),
            notes,
        );
    };

    let matches = declared.get(&key).map_or(0, Vec::len);
    let registered = registrations.get(&key).copied().unwrap_or(0);
    if matches != 1 || registered != 1 {
        let why = if matches == 0 {
            format!(
                "no {} in this image's contract carries type `{}` on `{source}`. The contract \
                 describes: {}",
                row.kind.tag(),
                key.1,
                describe(&of_kind),
            )
        } else {
            format!(
                "`{}` on `{source}` is declared by {matches} contract endpoint(s) and registered \
                 {registered} time(s) by component `{component}`, so which declaration governs \
                 THIS registration is ambiguous. A depth published against the wrong endpoint is \
                 an UNDER-size, so the match is refused rather than picked (RFC-0100 D6)",
                key.1,
            )
        };
        return refused(row, why, notes);
    }

    claimed.insert((component.to_string(), key.clone()));
    let c = &declared[&key][0];
    EntityDecl {
        // The contract's own spelling of the name wins from here on: it is the
        // RESOLVED name, which is what every consumer of this row means by
        // `topic`.
        name: c.name.clone(),
        depth: c.depth,
        reliability: c.reliability,
        durability: c.durability,
        history: c.history,
        buffer: c.buffer,
        publish_rate: c.publish_rate,
        drain_rate: c.drain_rate,
        contract_refusal: None,
        ..row.clone()
    }
}

/// The contract key a kind is declared under, for the remedy line.
fn contract_block(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Publisher => "pub",
        EntityKind::Subscription => "sub",
        EntityKind::ServiceServer | EntityKind::ServiceClient => "srv",
        EntityKind::ActionServer | EntityKind::ActionClient => "action",
        EntityKind::Timer | EntityKind::GuardCondition => "<none>",
    }
}

fn describe(candidates: &[&EntityDecl]) -> String {
    if candidates.is_empty() {
        return "nothing of this kind".to_string();
    }
    candidates
        .iter()
        .map(|c| {
            format!(
                "`{}` on `{}`",
                c.type_name.as_deref().unwrap_or("<untyped>"),
                c.name.as_deref().unwrap_or("<unnamed>")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Mark a row unattributable, recording the reason on the row AND in the notes.
///
/// Both, deliberately: the note reaches the person running the build, and the
/// row's own reason reaches the descriptor, which is where the consumer that
/// would otherwise have defaulted silently reads it.
fn refused(row: &EntityDecl, why: String, notes: &mut Vec<String>) -> EntityDecl {
    notes.push(format!(
        "{} `{}`: {why}",
        row.kind.tag(),
        row.source_topic
            .as_deref()
            .or(row.name.as_deref())
            .unwrap_or("<unnamed>"),
    ));
    EntityDecl {
        contract_refusal: Some(why),
        ..row.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_orchestration_ir::qos_override::QoSHistoryPolicy;
    use ros_launch_manifest_model::SystemModel;

    const QOS_DEPTH_1: &str = "      qos:\n        history: keep_last\n        depth: 1";

    /// One node, one topic it subscribes to, with a contract QoS on the
    /// subscription.
    ///
    /// Built as YAML and parsed rather than by poking struct fields, so the
    /// test exercises the same shape `nros-launch-resolve` writes.
    fn model_yaml(node: &str, topic: &str, ty: &str, remap: &str) -> SystemModel {
        let text = format!(
            "meta:\n\
             \x20 version: 1\n\
             structure:\n\
             \x20 nodes:\n\
             \x20   /{node}:\n\
             \x20     scope: system.launch.xml\n\
             \x20     pkg: demo\n\
             \x20     exec: {node}\n\
             \x20     node_name: {node}\n\
             {remap}\
             \x20 topics:\n\
             \x20   {topic}:\n\
             \x20     type: {ty}\n\
             \x20     sub:\n\
             \x20     - /{node}/ep\n\
             contracts:\n\
             \x20 sub_endpoints:\n\
             \x20   /{node}/ep:\n\
             {QOS_DEPTH_1}\n"
        );
        SystemModel::from_yaml_str(&text).expect("fixture model parses")
    }

    fn probe_sub(ty: &str, callback: &str, written: Option<&str>) -> EntityDecl {
        EntityDecl {
            source_topic: written.map(str::to_string),
            ..EntityDecl::bare(
                EntityKind::Subscription,
                Some(ty.into()),
                Some(callback.into()),
            )
        }
    }

    fn probe(component: &str, rows: Vec<EntityDecl>) -> EntityInventory {
        let mut inv = EntityInventory::new("metadata");
        inv.insert(ComponentEntities {
            pkg: "demo".into(),
            component: component.into(),
            class: component.into(),
            declaration: Declaration::Stated(rows),
        });
        inv
    }

    fn rows_of(inv: &EntityInventory) -> Vec<EntityDecl> {
        inv.components()[0].declaration.entities().to_vec()
    }

    /// THE join, on the shape the acceptance measures: the probe knows a
    /// subscription exists and calls it `on_chatter`; the contract knows
    /// `/chatter` keeps one sample.
    #[test]
    fn the_written_topic_joins_a_callback_named_row_to_its_contract_endpoint() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/String",
                "on_chatter",
                Some("/chatter"),
            )],
        );
        let j = join(&p, &m);
        assert!(j.contract_seen);
        let row = &rows_of(&j.inventory)[0];
        assert_eq!(row.depth, Some(1), "the contract's depth reached the row");
        assert_eq!(row.history, Some(QoSHistoryPolicy::KeepLast));
        // And the row's NAME is now the resolved topic, not the callback: the
        // descriptor's column is called `topic` and this is what it means.
        assert_eq!(row.name.as_deref(), Some("/chatter"));
        assert!(row.contract_refusal.is_none());
        assert!(j.notes.is_empty(), "{:?}", j.notes);
    }

    /// MUTATION CONTROL for the key: a probe row whose written topic is NOT the
    /// contract's REFUSES, named, rather than taking the only candidate.
    ///
    /// This is the test that reds if the key regresses to "one kind, one
    /// candidate, take it" — which is exactly the mis-attribution that
    /// publishes a depth against the wrong endpoint.
    #[test]
    fn a_row_on_a_different_topic_refuses_rather_than_taking_the_lone_candidate() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/String",
                "on_status",
                Some("/status"),
            )],
        );
        let j = join(&p, &m);
        let row = &rows_of(&j.inventory)[0];
        assert_eq!(row.depth, None, "no depth may be attributed");
        let why = row.contract_refusal.clone().expect("refused");
        assert!(why.contains("/status"), "{why}");
        assert!(
            why.contains("/chatter"),
            "names what the contract HAS: {why}"
        );
        assert!(
            j.notes.iter().any(|n| n.contains("/status")),
            "{:?}",
            j.notes
        );
    }

    /// The same for the TYPE half of the key.
    #[test]
    fn a_row_of_a_different_type_refuses_rather_than_matching_on_topic_alone() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/Int32",
                "on_chatter",
                Some("/chatter"),
            )],
        );
        let j = join(&p, &m);
        let row = &rows_of(&j.inventory)[0];
        assert_eq!(row.depth, None);
        assert!(
            row.contract_refusal
                .clone()
                .unwrap()
                .contains("std_msgs/msg/Int32")
        );
    }

    /// A relative spelling is NOT resolved here. `chatter` and `/chatter` name
    /// the same endpoint only in the root namespace with no remaps, and this
    /// reader is told neither per endpoint.
    #[test]
    fn a_relative_topic_refuses_rather_than_being_resolved() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/String",
                "on_chatter",
                Some("chatter"),
            )],
        );
        let j = join(&p, &m);
        let row = &rows_of(&j.inventory)[0];
        assert_eq!(row.depth, None);
        let why = row.contract_refusal.clone().unwrap();
        assert!(why.contains("not an absolute name"), "{why}");
    }

    /// A node the launch description REMAPS refuses every one of its rows,
    /// absolute spellings included: a remap can rename any name, and the model
    /// does not say which endpoint it applied to.
    #[test]
    fn a_remapped_node_refuses_even_an_absolute_topic() {
        let m = model_yaml(
            "listener",
            "/chatter",
            "std_msgs/msg/String",
            "      remaps:\n      - from: /chatter\n        to: /other\n",
        );
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/String",
                "on_chatter",
                Some("/chatter"),
            )],
        );
        let j = join(&p, &m);
        let row = &rows_of(&j.inventory)[0];
        assert_eq!(row.depth, None);
        assert!(row.contract_refusal.clone().unwrap().contains("REMAPPINGS"));
    }

    /// Two registrations on one topic against one contract endpoint: BOTH
    /// refuse.
    ///
    /// The under-description case. Attributing the declaration to whichever
    /// registration the probe listed first would publish a depth for a
    /// registration nobody declared, which is why the uniqueness rule counts
    /// BOTH sides and not just the contract's.
    #[test]
    fn two_registrations_against_one_declaration_both_refuse() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe(
            "listener",
            vec![
                probe_sub("std_msgs/msg/String", "a", Some("/chatter")),
                probe_sub("std_msgs/msg/String", "b", Some("/chatter")),
            ],
        );
        let j = join(&p, &m);
        let rows = rows_of(&j.inventory);
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert_eq!(r.depth, None, "neither row may claim the declaration");
            assert!(r.contract_refusal.clone().unwrap().contains("ambiguous"));
        }
    }

    /// NO CONTRACT: nothing happens, and that is the byte-identity acceptance.
    #[test]
    fn a_model_with_no_wiring_returns_the_probe_inventory_untouched() {
        let m = SystemModel::from_yaml_str(
            "meta:\n  version: 1\nstructure:\n  nodes:\n    /listener:\n      scope: \
             system.launch.xml\n      pkg: demo\n      exec: listener\n      node_name: \
             listener\n",
        )
        .expect("fixture model parses");
        let p = probe(
            "listener",
            vec![probe_sub(
                "std_msgs/msg/String",
                "on_chatter",
                Some("/chatter"),
            )],
        );
        let j = join(&p, &m);
        assert!(!j.contract_seen);
        assert_eq!(j.inventory, p, "untouched, row for row");
        assert!(j.notes.is_empty());
    }

    /// A contract endpoint the code does not create is REPORTED and refuses
    /// nothing: every attributed row was attributed from a key unique on both
    /// sides, so a leftover cannot have made one of them wrong.
    #[test]
    fn a_contract_endpoint_with_no_registration_is_reported_not_refused() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let p = probe("listener", vec![]);
        let j = join(&p, &m);
        assert!(
            j.notes
                .iter()
                .any(|n| n.contains("creates no such endpoint")),
            "{:?}",
            j.notes
        );
    }

    /// A TIMER passes straight through: it subscribes to nothing, so it has no
    /// identity to key on and no QoS to gain, and refusing it would report a
    /// problem that does not exist.
    #[test]
    fn a_timer_is_neither_attributed_nor_refused() {
        let m = model_yaml("listener", "/chatter", "std_msgs/msg/String", "");
        let mut rows = vec![EntityDecl::bare(
            EntityKind::Timer,
            None,
            Some("on_tick".into()),
        )];
        rows.push(probe_sub(
            "std_msgs/msg/String",
            "on_chatter",
            Some("/chatter"),
        ));
        let j = join(&probe("listener", rows), &m);
        let out = rows_of(&j.inventory);
        assert_eq!(out[0].kind, EntityKind::Timer);
        assert!(out[0].contract_refusal.is_none());
        assert_eq!(out[1].depth, Some(1), "its neighbour still joined");
    }
}
