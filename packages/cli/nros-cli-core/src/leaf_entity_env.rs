//! Issue 0827 — a cargo leaf's pool budgets, derived from what it declares.
//!
//! The pools that dominate static RAM (`ZPICO_MAX_SUBSCRIBERS`,
//! `ZPICO_MAX_QUERYABLES`, `ZPICO_MAX_PUBLISHERS`) are sized in
//! `nros-rmw-zenoh`'s build script. That crate is a DEPENDENCY of the leaf,
//! and the entities are declared in the leaf's own `src/lib.rs`, so the crate
//! that must know the counts compiles BEFORE the crate whose source states
//! them. No build script, proc macro or manifest key can reach backwards
//! across that edge.
//!
//! It does not have to. The probe already ran: `nros sync` writes
//! `<leaf>/metadata/<component>.json` describing what the component creates.
//! This module turns that file into the same [`EntityDecl`] rows a
//! `nano_ros_node_register(... ENTITIES ...)` would have produced, so the
//! COUNTING RULES stay in [`crate::entity_inventory`] where CMake consumers
//! already get them. A second derivation would be a second answer.
//!
//! Both ends are per-host artifacts and neither is committed: the probe output
//! is gitignored (`examples/**/metadata/*.json`), and so is the `[env]` sidecar
//! this renders. A fresh clone has neither and gets both from `nros sync` — the
//! contract `generated/` already has.
//!
//! What this does NOT reach: a leaf whose metadata is `<component>.json
//! .unprobeable`. The probe cannot run for a foreign `[build] target` with
//! `[unstable] build-std`, nor for a component whose board crate has no host
//! build. Those leaves state their budgets by hand and that is issue 1061.

use std::{collections::BTreeMap, path::Path};

use eyre::{Result, WrapErr};
use serde::Deserialize;

use crate::entity_inventory::{
    ComponentEntities, Declaration, EntityDecl, EntityInventory, EntityKind,
};

/// The probe's per-node lists, and the [`EntityKind`] each one means.
///
/// One table, so a kind cannot be silently dropped by being handled in one
/// place and forgotten in another. `guard_condition` has no probe key — the
/// Rust node metadata does not describe one — and its absence here is
/// deliberate rather than an oversight.
const NODE_ENTITY_KEYS: &[(&str, EntityKind)] = &[
    ("publishers", EntityKind::Publisher),
    ("subscribers", EntityKind::Subscription),
    ("timers", EntityKind::Timer),
    ("services", EntityKind::ServiceServer),
    ("service_clients", EntityKind::ServiceClient),
    ("actions", EntityKind::ActionServer),
    ("action_clients", EntityKind::ActionClient),
];

#[derive(Debug, Deserialize)]
struct ProbeInterface {
    package: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeEntity {
    id: Option<String>,
    interface: Option<ProbeInterface>,
}

#[derive(Debug, Deserialize)]
struct ProbeNode {
    #[serde(flatten)]
    lists: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ProbeDoc {
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    nodes: Vec<ProbeNode>,
}

/// `example_interfaces` + `action/Fibonacci` -> `example_interfaces/action/Fibonacci`.
fn qualified_type(iface: &ProbeInterface) -> Option<String> {
    match (iface.package.as_deref(), iface.name.as_deref()) {
        (Some(p), Some(n)) => Some(format!("{p}/{n}")),
        _ => None,
    }
}

/// Turn ONE probe document into the rows the inventory counts.
pub fn declaration_from_probe(doc_json: &str) -> Result<(String, String, Declaration)> {
    let doc: ProbeDoc =
        serde_json::from_str(doc_json).wrap_err("leaf metadata is not the probe's JSON shape")?;
    let pkg = doc.package.clone().unwrap_or_else(|| "<unknown>".into());
    let comp = doc.component.clone().unwrap_or_else(|| "<unknown>".into());

    let mut decls: Vec<EntityDecl> = Vec::new();
    for node in &doc.nodes {
        for (key, kind) in NODE_ENTITY_KEYS {
            let Some(v) = node.lists.get(*key) else {
                continue;
            };
            let Some(arr) = v.as_array() else { continue };
            for item in arr {
                let ent: ProbeEntity = match serde_json::from_value(item.clone()) {
                    Ok(e) => e,
                    // A row this module cannot read is NOT skipped quietly: it
                    // would lower a pool below what the image creates, and short
                    // halts the board. Refuse the whole leaf instead.
                    Err(e) => {
                        return Err(eyre::eyre!(
                            "leaf metadata has a `{key}` entry this module cannot read ({e}); \
                             refusing rather than deriving a budget that is short"
                        ));
                    }
                };
                decls.push(EntityDecl {
                    kind: *kind,
                    type_name: ent.interface.as_ref().and_then(qualified_type),
                    name: ent.id,
                    depth: None,
                });
            }
        }
    }

    // `Stated(vec![])` and `None` are the same COUNT and different FACTS: the
    // probe ran and found nothing, versus a component that never declared. The
    // probe running IS a statement, so an empty result is `None` (asserts it
    // creates nothing), never `Absent`.
    let declaration = if decls.is_empty() {
        Declaration::None
    } else {
        Declaration::Stated(decls)
    };
    Ok((pkg, comp, declaration))
}

/// Every probeable component under `<leaf>/metadata/`, as one image's inventory.
///
/// `.json.unprobeable` files are skipped BY NAME and reported, because their
/// existence is the reason a leaf may get no sidecar at all — a silent skip
/// would render a budget for half an image.
pub fn inventory_for_leaf(leaf: &Path) -> Result<(EntityInventory, Vec<String>)> {
    let dir = leaf.join("metadata");
    let mut inv = EntityInventory::new(dir.display().to_string());
    let mut unprobeable = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Ok((inv, unprobeable));
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if name.ends_with(".json.unprobeable") {
            unprobeable.push(name.to_string());
            continue;
        }
        if !name.ends_with(".json") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .wrap_err_with(|| format!("reading {}", path.display()))?;
        let (pkg, component, declaration) = declaration_from_probe(&text)
            .wrap_err_with(|| format!("parsing {}", path.display()))?;
        inv.insert(ComponentEntities {
            pkg,
            component: component.clone(),
            class: component,
            declaration,
        });
    }
    Ok((inv, unprobeable))
}

/// The knobs a derived budget sets, and which field answers each.
///
/// Names are the ones the BUILD SCRIPTS read, checked against them rather than
/// guessed: `ZPICO_*` are `zpico-sys`/`nros-zpico-build`, `NROS_RMW_*` is
/// `nros-rmw-cffi`, `NROS_EXECUTOR_*` is `nros-node`.
pub const DERIVED_ENV_KEYS: &[&str] = &[
    "NROS_EXECUTOR_ACTION_CLIENTS",
    "NROS_EXECUTOR_MAX_CBS",
    "NROS_RMW_SUBSCRIBER_SLOTS",
    "ZPICO_MAX_PUBLISHERS",
    "ZPICO_MAX_QUERYABLES",
    "ZPICO_MAX_SUBSCRIBERS",
];

/// Render the gitignored `[env]` sidecar for a derived budget.
pub fn render_env_sidecar(
    knobs: &crate::entity_inventory::DerivedEntityKnobs,
    source: &str,
) -> String {
    let mut s = String::new();
    s.push_str("# GENERATED by `nros sync` (issue 0827) — DO NOT EDIT.\n#\n");
    s.push_str(
        "# Pool budgets derived from what this leaf's components DECLARE, read\n\
         # from the metadata probe's output. The counting rules are\n\
         # `nros_cli_core::entity_inventory` — the same ones a CMake image gets,\n\
         # so a cargo leaf and a configured image cannot disagree.\n#\n",
    );
    s.push_str(
        "# NOT committed, and it must not be: it is derived from a probe output\n\
         # that is itself per-host and gitignored. A fresh clone regenerates both\n\
         # with `nros sync`.\n#\n",
    );
    s.push_str(&format!("# source: {source}\n"));
    s.push_str(
        "#\n# An environment value the caller sets WINS over this file: cargo's `[env]`\n\
         # does not override an already-set variable unless `force = true`, which\n\
         # this deliberately does not use. A number a human states beats a number\n\
         # derived on their behalf.\n\n",
    );
    s.push_str("[env]\n");
    let vals: BTreeMap<&str, usize> = BTreeMap::from([
        ("NROS_EXECUTOR_ACTION_CLIENTS", knobs.heavy_slots),
        ("NROS_EXECUTOR_MAX_CBS", knobs.max_cbs),
        ("NROS_RMW_SUBSCRIBER_SLOTS", knobs.max_subscribers),
        ("ZPICO_MAX_PUBLISHERS", knobs.max_publishers),
        ("ZPICO_MAX_QUERYABLES", knobs.max_queryables),
        ("ZPICO_MAX_SUBSCRIBERS", knobs.max_subscribers),
    ]);
    for (k, v) in &vals {
        s.push_str(&format!("{k} = \"{v}\"\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_inventory::Derivation;

    const TALKER: &str = r#"{
      "version": 1, "package": "native_talker", "component": "talker",
      "nodes": [{ "id": "talker",
        "publishers": [{"id": "/chatter",
          "interface": {"package": "std_msgs", "name": "msg/String", "kind": "message"}}],
        "timers": [{"id": "on_tick"}],
        "subscribers": [], "services": [], "actions": [] }]
    }"#;

    #[test]
    fn a_publisher_and_a_timer_become_two_decls() {
        let (pkg, comp, d) = declaration_from_probe(TALKER).unwrap();
        assert_eq!((pkg.as_str(), comp.as_str()), ("native_talker", "talker"));
        let ents = d.entities();
        assert_eq!(ents.len(), 2, "{ents:?}");
        assert_eq!(ents[0].kind, EntityKind::Publisher);
        assert_eq!(ents[0].type_name.as_deref(), Some("std_msgs/msg/String"));
        assert_eq!(ents[0].name.as_deref(), Some("/chatter"));
        assert_eq!(ents[1].kind, EntityKind::Timer);
    }

    /// The probe running and finding nothing is a STATEMENT, not an absence.
    #[test]
    fn an_empty_probe_is_none_not_absent() {
        let (_, _, d) = declaration_from_probe(
            r#"{"package":"p","component":"c","nodes":[{"id":"n","publishers":[]}]}"#,
        )
        .unwrap();
        assert_eq!(d.tag(), "none");
        assert!(d.entities().is_empty());
    }

    /// Every key in the table reaches a kind. A kind handled here and forgotten
    /// there is how a pool silently goes short.
    #[test]
    fn every_probe_key_maps_to_a_kind() {
        let doc = r#"{"package":"p","component":"c","nodes":[{
          "publishers":[{"id":"a"}], "subscribers":[{"id":"b"}], "timers":[{"id":"c"}],
          "services":[{"id":"d"}], "service_clients":[{"id":"e"}],
          "actions":[{"id":"f"}], "action_clients":[{"id":"g"}] }]}"#;
        let (_, _, d) = declaration_from_probe(doc).unwrap();
        let kinds: Vec<_> = d.entities().iter().map(|e| e.kind).collect();
        for (_, want) in NODE_ENTITY_KEYS {
            assert!(
                kinds.contains(want),
                "{want:?} never produced by {NODE_ENTITY_KEYS:?}"
            );
        }
        assert_eq!(kinds.len(), NODE_ENTITY_KEYS.len());
    }

    /// An unreadable row REFUSES. Deriving a budget from a partial read is how
    /// a pool ends up shorter than the image, and short halts the board.
    #[test]
    fn an_unreadable_entity_row_refuses() {
        let doc = r#"{"package":"p","component":"c","nodes":[{"publishers":[42]}]}"#;
        let err = declaration_from_probe(doc).unwrap_err().to_string();
        assert!(err.contains("cannot read"), "{err}");
    }

    /// The derivation is the SHARED one, not a copy: a talker's publisher
    /// claims no callback slot and its timer does, which is `entity_inventory`'s
    /// rule and not this module's.
    #[test]
    fn counts_flow_through_the_shared_derivation() {
        let (pkg, comp, d) = declaration_from_probe(TALKER).unwrap();
        let mut inv = EntityInventory::new("t");
        inv.insert(ComponentEntities {
            pkg,
            component: comp.clone(),
            class: comp,
            declaration: d,
        });
        let Derivation::Derived(k) = inv.derive() else {
            panic!("expected a derivation from a stated declaration");
        };
        assert_eq!(k.entity_total, 2);
        assert_eq!(k.max_publishers, 1);
        // ONE, not zero, and that is `entity_inventory`'s rule rather than a
        // rounding-up here: issue 1015 puts a FLOOR OF ONE on any pool backing
        // a fixed C array, because `queryable_entry_t queryables[0]` is not a
        // smaller pool, it is a different kind of object. This assertion
        // originally read 0 — the shared derivation corrected it, which is the
        // whole reason this module reuses it instead of counting for itself.
        assert_eq!(
            k.max_subscribers, 1,
            "floored at one (issue 1015), not the raw 0"
        );
        assert_eq!(
            k.max_cbs, 1,
            "the timer claims the slot; the publisher does not"
        );
    }

    #[test]
    fn the_sidecar_states_every_declared_key() {
        let (pkg, comp, d) = declaration_from_probe(TALKER).unwrap();
        let mut inv = EntityInventory::new("t");
        inv.insert(ComponentEntities {
            pkg,
            component: comp.clone(),
            class: comp,
            declaration: d,
        });
        let Derivation::Derived(k) = inv.derive() else {
            panic!()
        };
        let out = render_env_sidecar(&k, "metadata/talker.json");
        assert!(out.contains("[env]"));
        for key in DERIVED_ENV_KEYS {
            assert!(out.contains(key), "sidecar omits {key}:\n{out}");
        }
        assert!(out.contains("ZPICO_MAX_SUBSCRIBERS = \"1\""), "{out}");
        assert!(out.contains("ZPICO_MAX_PUBLISHERS = \"1\""), "{out}");
        // No `force = true` KEY: a value the caller states must win. Checked
        // line-wise, because the header prose explains `force` and a substring
        // test would match the explanation rather than a setting.
        assert!(
            !out.lines().any(|l| l.trim_start().starts_with("force")),
            "sidecar sets `force`, so a caller's own value would be overridden:\n{out}"
        );
    }
}
