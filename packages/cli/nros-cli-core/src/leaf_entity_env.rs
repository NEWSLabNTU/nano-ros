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
//! A leaf whose metadata is `<component>.json.unprobeable` cannot be probed at
//! all — the probe compiles the component for the HOST, and neither a foreign
//! `[build] target` with `[unstable] build-std` nor a board crate with no host
//! build allows that. Issue 1061.
//!
//! For those, the leaf DECLARES instead, in its own manifest:
//!
//! ```toml
//! [package.metadata.nros.component]
//! entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]
//! ```
//!
//! Same grammar as `nano_ros_node_register(... ENTITIES ...)`, parsed by the
//! same [`EntityDecl::parse`], because a second spelling of one declaration is
//! how the two drift. Nothing is compiled to read it.
//!
//! **A declaration is CROSS-CHECKED against the probe wherever the probe can
//! run.** That is what keeps a hand-written list from quietly going stale: on a
//! probeable leaf the two must agree, and a mismatch REFUSES rather than
//! picking one. The declaration exists for leaves where there is nothing to
//! check it against; it does not become a way to override what the code says.

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
                // No QoS at all on this road: the leaf probe reports what the
                // code CREATES, and a `create_subscription(qos)` argument is a
                // runtime value this metadata never carried. The contract file
                // is the QoS surface (RFC-0100 D3).
                decls.push(EntityDecl::bare(
                    *kind,
                    ent.interface.as_ref().and_then(qualified_type),
                    ent.id,
                ));
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

/// Issue 1061 / RFC-0098 D8 — the entities a leaf DECLARES.
///
/// `entities = [...]` on the leaf's `system.toml` `[[component]]` rows (the
/// retiring `[package.metadata.nros.component] entities` is still read, through
/// the same reader's fallback), each string in the
/// `nano_ros_node_register(... ENTITIES ...)` grammar. `Ok(None)` means nothing
/// is declared, which is different from an empty list: an empty list is a leaf
/// asserting it creates nothing.
pub fn declared_entities(leaf: &Path) -> Result<Option<Vec<EntityDecl>>> {
    let Some(decl) = nros_orchestration_ir::leaf_system::read(leaf).map_err(|e| eyre::eyre!(e))?
    else {
        return Ok(None);
    };
    let Some(specs) = decl.declared_entities() else {
        return Ok(None);
    };
    let origin = decl.origin_path().display().to_string();
    let mut out = Vec::new();
    for spec in &specs {
        // The SAME parser the CMake path uses. A private grammar here is how a
        // declaration means one thing in a CMakeLists and another in a manifest.
        let decls = EntityDecl::parse(spec)
            .map_err(|e| eyre::eyre!("{origin}: entities entry `{spec}`: {e}"))?;
        out.extend(decls);
    }
    Ok(Some(out))
}

/// Compare a declaration with what the probe found, as multisets of KIND.
///
/// Kind only, deliberately. The probe resolves topic names and interfaces that
/// a hand-written declaration may legitimately state loosely (`timer` carries
/// neither), so comparing the full row would refuse honest declarations. What a
/// budget is computed from is the per-kind COUNT, so that is what has to agree —
/// a mismatch there is a mismatch in the numbers this module exists to produce.
fn kind_counts(decls: &[EntityDecl]) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for d in decls {
        *m.entry(d.kind.tag()).or_insert(0) += 1;
    }
    m
}

/// `Err` when a declaration and a successful probe disagree.
pub fn reconcile(component: &str, declared: &[EntityDecl], probed: &[EntityDecl]) -> Result<()> {
    let (d, p) = (kind_counts(declared), kind_counts(probed));
    if d == p {
        return Ok(());
    }
    let fmt = |m: &BTreeMap<&'static str, usize>| {
        if m.is_empty() {
            "nothing".to_string()
        } else {
            m.iter()
                .map(|(k, v)| format!("{k}x{v}"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    Err(eyre::eyre!(
        "{component}: the leaf declares {} but the code creates {}.\n  \
         Refusing rather than choosing one: a budget from the declaration would be \
         wrong for the image, and silently preferring the probe would let the \
         declaration rot until it reaches a leaf where nothing can check it.\n  \
         Fix the `entities` list on the leaf's `system.toml` `[[component]]` (or the \
         retiring `[package.metadata.nros.component] entities`), or drop it and \
         let the probe answer.",
        fmt(&d),
        fmt(&p)
    ))
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
    // Kept beside the inventory so the reconcile below can read what was probed
    // without `EntityInventory` growing an accessor for one caller.
    let mut probed_rows: Vec<(String, Vec<EntityDecl>)> = Vec::new();
    // Issue 1061 — what the leaf DECLARES, if anything. Read before the probe
    // results so it can serve both roles: the answer where nothing was probed,
    // and the cross-check where something was.
    let declared = declared_entities(leaf)?;

    let Ok(rd) = std::fs::read_dir(&dir) else {
        // No `metadata/` at all. A declaration still answers — that is the whole
        // point for a leaf the probe cannot reach.
        if let Some(d) = declared {
            inv.insert(ComponentEntities {
                pkg: leaf_name(leaf),
                component: leaf_name(leaf),
                class: leaf_name(leaf),
                declaration: if d.is_empty() {
                    Declaration::None
                } else {
                    Declaration::Stated(d)
                },
            });
        }
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
        probed_rows.push((component.clone(), declaration.entities().to_vec()));
        inv.insert(ComponentEntities {
            pkg,
            component: component.clone(),
            class: component,
            declaration,
        });
    }
    // Issue 1061 — reconcile, or stand in.
    match (&declared, inv.is_empty()) {
        // Probed AND declared: they must agree. Checked per component only when
        // there is exactly one, because a multi-component leaf's manifest states
        // one list for the whole package and splitting it across components
        // would be inventing an attribution.
        (Some(d), false) => {
            if let [(component, probed)] = probed_rows.as_slice() {
                reconcile(component, d, probed)?;
            }
        }
        // Declared with nothing probed: the declaration IS the answer. This is
        // the unprobeable leaf, which is what issue 1061 is about.
        (Some(d), true) => {
            inv.insert(ComponentEntities {
                pkg: leaf_name(leaf),
                component: leaf_name(leaf),
                class: leaf_name(leaf),
                declaration: if d.is_empty() {
                    Declaration::None
                } else {
                    Declaration::Stated(d.clone())
                },
            });
        }
        (None, _) => {}
    }
    Ok((inv, unprobeable))
}

/// The leaf directory's own name, used as pkg/component when a declaration
/// stands in for a probe that never ran and there is no probed name to use.
fn leaf_name(leaf: &Path) -> String {
    leaf.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("<leaf>")
        .to_string()
}

/// The knobs a derived budget sets, and which field answers each.
///
/// Names are the ones the BUILD SCRIPTS read, checked against them rather than
/// guessed: `ZPICO_*` are `zpico-sys`/`nros-zpico-build`, `NROS_RMW_*` is
/// `nros-rmw-cffi`, `NROS_EXECUTOR_*` is `nros-node`.
pub const DERIVED_ENV_KEYS: &[&str] = &[
    "NROS_EXECUTOR_ACTION_CLIENTS",
    "NROS_EXECUTOR_MAX_CBS",
    // issue 1233 — the node table, on the leaf road for the first time. Its
    // ceiling failure names the knob (`NodeError::NodeTableFull`), which is the
    // property phase-412 required before deriving it at all, and that property
    // belongs to the failure rather than to the road it travelled.
    "NROS_EXECUTOR_MAX_NODES",
    // issue 1198 — the executor's OTHER fixed table, which travelled on NO
    // road: 8 scheduling-context slots in every image whatever its schedule
    // created, and the Rust runtime creates NONE of them. Same terms as its
    // sibling above: a DEFAULT (cargo `[env]` without `force`, so a stated
    // value still wins), and exhaustion NAMES the knob
    // (`NodeError::NoSchedContextSlot`).
    "NROS_EXECUTOR_MAX_SC",
    "NROS_RMW_SUBSCRIBER_SLOTS",
    // issue 1130 — the per-kind capacity of a knob-capped component cell. An
    // explicit `ENTITY_BOUNDS` on a class still wins: it is per CLASS.
    "NROS_RUNTIME_MAX_CELL_ENTITIES",
    "ZPICO_MAX_PUBLISHERS",
    "ZPICO_MAX_SUBSCRIBERS",
];

/// Issue 1125 — the PAYLOAD-CLASS keys, which come from a second inventory.
///
/// Kept apart from [`DERIVED_ENV_KEYS`] because they empty independently and
/// for different reasons: the entity budget needs only the probe, while these
/// need the message-bound artifacts too and REFUSE when a subscribed type
/// carries no bound. A leaf can legitimately get one group and not the other,
/// so a single list would have to be "sometimes present", which is what makes
/// an absent key unreadable.
///
/// `ZPICO_SUBSCRIBER_LARGE_SIZE` is here and is emitted only when the large
/// count is non-zero — with zero blocks the pool is zero bytes whatever the
/// size says, and naming a size for a class that does not exist would be
/// inventing a number. Same rule as `_nros_bounds_publish_payload_classes`.
pub const DERIVED_PAYLOAD_ENV_KEYS: &[&str] = &[
    "NROS_SUBSCRIBER_BUFFER_SIZE",
    "ZPICO_MAX_LARGE_SUBSCRIBERS",
    "ZPICO_SUBSCRIBER_LARGE_SIZE",
];

/// Issue 1233 — the CLOSURE-basis key, which is a THIRD group for a third
/// reason.
///
/// [`DERIVED_PAYLOAD_ENV_KEYS`] is derived over what the leaf SUBSCRIBES to;
/// this one may not be. `NROS_SUBSCRIPTION_BUFFER_SIZE` is one global `RX_BUF`
/// for every entity and is also `DEFAULT_TX_BUF`, so a type the leaf only
/// PUBLISHES still has to fit — the same rule `NanoRosMessageBounds.cmake`
/// states at the derivation ("BASIS `closure`, always. Narrowing this one is
/// the under-derivation."). It therefore refuses on inputs the payload join
/// accepts, and would be unreadable inside either list above.
pub const DERIVED_CLOSURE_ENV_KEYS: &[&str] = &["NROS_SUBSCRIPTION_BUFFER_SIZE"];

/// `ZPICO_MAX_QUERYABLES` is never stated here as a COUNT: the consumer
/// derives it from the FACTS this road carries (RFC-0098 D7, phase-445 W1).
///
/// `nros-zpico-build` sizes the queryable table from
/// `NROS_DECLARED_SERVICE_SERVERS` + `NROS_DECLARED_INFRA_QUERYABLES` +
/// `NROS_DECLARED_NODES`, adding the parameter and lifecycle costs defined
/// beside the code that registers them. Carrying the facts rather than the
/// count is issue 0460's rule: a count computed here would have to restate
/// those costs, and a second statement of them is how they drift.
///
/// Until phase-445 W1 this was `NOT_DERIVED_NEEDS_INFRA_COUNT`, and the knob
/// was simply withheld: the inventory this road builds comes from the probe or
/// a `[[component]] entities` declaration, which cannot see whether the image
/// carries the parameter and lifecycle service families, so a bare count was
/// SHORT for any image with param services — a registration failure at boot,
/// not a smaller pool. The esp32 leaves hand-set it to 2 to fit DRAM. The
/// facts close that gap ([`leaf_facts`]): the infrastructure half comes from
/// the leaf's resolved model UNIONED with the service features its manifest
/// enables, and the application half from the same inventory every other
/// derived knob here comes from. A leaf with no readable model gets no facts,
/// and the consumer keeps its undeclared budget (8 embedded / 32 hosted) —
/// large, never short.
const QUERYABLES_DERIVED_BY_CONSUMER: &str = "ZPICO_MAX_QUERYABLES";

/// `ZPICO_MAX_LIVELINESS` is not stated here either — with the same cause as
/// the knob above and WITHOUT its remedy.
///
/// `DerivedEntityKnobs::max_liveliness` is one token per session entity, and
/// every parameter or lifecycle service server is a session entity: it
/// declares a token exactly as an application server does. This road's
/// inventory cannot see those families, so the count would be SHORT for any
/// image carrying them. Short is not a boot failure here -- the entity works
/// and is invisible to `ros2 node list`, with a log line naming the knob -- but
/// it is the silent graph outage issue 0283 exists to prevent, and the crate
/// default (16) is larger and safe.
///
/// The difference from [`QUERYABLES_DERIVED_BY_CONSUMER`] is the whole reason
/// these are two constants and not one. That knob is withheld as a COUNT and
/// completed by `nros-zpico-build` from the `NROS_DECLARED_*` facts this road
/// does carry; no consumer completes the liveliness pool from facts, so there
/// is nothing to hand it and it keeps the zpico default outright. The Zephyr
/// resolver road derives it, from an inventory composed with the model.
const NOT_DERIVED_LIVELINESS_NEEDS_INFRA_COUNT: &str = "ZPICO_MAX_LIVELINESS";

/// Render the gitignored `[env]` sidecar for a derived budget.
pub fn render_env_sidecar(
    knobs: &crate::entity_inventory::DerivedEntityKnobs,
    payload: &crate::leaf_payload_classes::PayloadClasses,
    take: &crate::leaf_take_buffer::TakeBuffer,
    source: &str,
) -> String {
    render_env_sidecar_with_facts(knobs, payload, take, source, &BTreeMap::new())
}

/// [`render_env_sidecar`], plus the image's `NROS_DECLARED_*` facts as `[env]`
/// rows (phase-445 W1). Empty `facts` renders exactly what
/// [`render_env_sidecar`] always has.
pub fn render_env_sidecar_with_facts(
    knobs: &crate::entity_inventory::DerivedEntityKnobs,
    payload: &crate::leaf_payload_classes::PayloadClasses,
    take: &crate::leaf_take_buffer::TakeBuffer,
    source: &str,
    facts: &BTreeMap<String, String>,
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
    if facts.contains_key("NROS_DECLARED_INFRA_QUERYABLES") {
        s.push_str(&format!(
            "# `{QUERYABLES_DERIVED_BY_CONSUMER}` is not stated as a count: the `NROS_DECLARED_*`\n"
        ));
        s.push_str("# facts at the end of this table are what `nros-zpico-build` sizes it\n");
        s.push_str("# from, adding the parameter and lifecycle costs itself (issue 0460).\n\n");
    } else {
        s.push_str(&format!(
            "# `{QUERYABLES_DERIVED_BY_CONSUMER}` is not stated — this leaf has no resolved model,\n"
        ));
        s.push_str("# so nothing says whether the image carries the parameter or lifecycle\n");
        s.push_str("# service families, and the consumer keeps its undeclared budget.\n\n");
    }
    // phase-412 W2 — the liveliness pool sits one step behind its queryable
    // sibling above and says so. Both counts need the parameter and lifecycle
    // families this road cannot see; the difference is that the queryable
    // count has a CONSUMER that completes it from the `NROS_DECLARED_*` facts,
    // and the liveliness pool has none, so it keeps the zpico default outright.
    s.push_str(&format!(
        "# `{NOT_DERIVED_LIVELINESS_NEEDS_INFRA_COUNT}` is not stated either, and has no such\n"
    ));
    s.push_str("# consumer-side completion: every one of those runtime servers also\n");
    s.push_str("# declares a liveliness token, so a count from this road would be short\n");
    s.push_str("# and nothing downstream could add the difference back.\n\n");
    s.push_str(
        "# The two `ZPICO_*` rows are FLOORED AT ONE: they size fixed C arrays in\n\
         # `zpico.c`, where zero is not a smaller pool (issue 1015). The floor is\n\
         # applied here, at the consumer, and not in the derivation -- the same\n\
         # derived counts reach the XRCE pools, where zero IS the answer and is\n\
         # worth 33,296 bytes of heap a slot (issue 1033).\n\n",
    );
    s.push_str("[env]\n");
    let floor = crate::entity_inventory::c_array_pool_floor;
    let vals: BTreeMap<&str, usize> = BTreeMap::from([
        ("NROS_EXECUTOR_ACTION_CLIENTS", knobs.heavy_slots),
        ("NROS_EXECUTOR_MAX_CBS", knobs.max_cbs),
        // issue 1233 — one node per declared component, unfloored like the
        // three counts around it: the floor belongs to the consumer that names
        // the knob, and this one sizes Rust tables where a short count is a
        // named `NodeTableFull`, not a `#error`.
        ("NROS_EXECUTOR_MAX_NODES", knobs.max_nodes),
        // issue 1198 — unfloored for the same reason, and it cannot reach zero
        // anyway: the derivation refuses an image with no components, so
        // `max_sc >= 2` (the reserved slot 0 plus that image's one).
        ("NROS_EXECUTOR_MAX_SC", knobs.max_sc),
        ("NROS_RMW_SUBSCRIBER_SLOTS", knobs.max_subscribers),
        // issue 1130 — unfloored: the cell registries are Rust arrays, and a
        // zero-capacity one is an empty registry, not a `#error`.
        ("NROS_RUNTIME_MAX_CELL_ENTITIES", knobs.max_cell_entities),
        ("ZPICO_MAX_PUBLISHERS", floor(knobs.max_publishers)),
        ("ZPICO_MAX_SUBSCRIBERS", floor(knobs.max_subscribers)),
    ]);
    for (k, v) in &vals {
        s.push_str(&format!("{k} = \"{v}\"\n"));
    }
    if !facts.is_empty() {
        s.push_str(
            "\n# The image's entity FACTS (RFC-0098 D7) — carried, never turned into a\n\
             # count here. See `leaf_entity_env::leaf_facts` for where each comes from.\n",
        );
        for (k, v) in facts {
            s.push_str(&format!("{k} = \"{v}\"\n"));
        }
    }

    // Issue 1125 — the payload classes, from the second inventory. Appended to
    // the same `[env]` table rather than given their own: cargo merges nothing
    // across tables, and a leaf reads one environment.
    match payload {
        crate::leaf_payload_classes::PayloadClasses::Derived(p) => {
            s.push('\n');
            s.push_str(&format!(
                "# Payload classes, derived over the {} subscribing entit{} this leaf\n\
                 # declares (issue 1125). `large_count = 0` is an ANSWER -- it says every\n\
                 # type this leaf receives fits the small class, and `LARGE_PAYLOADS`\n\
                 # becomes zero bytes instead of RING_DEPTH x LARGE_SIZE. It is NOT\n\
                 # floored: `alloc_payload_block` bounds-checks the class index before it\n\
                 # subscripts the pool, so a zero-length one is never indexed\n\
                 # (phase-403 W4, and the reason issue 1015's floor does not apply here).\n",
                p.subscribed,
                if p.subscribed == 1 { "y" } else { "ies" }
            ));
            s.push_str(&format!(
                "ZPICO_MAX_LARGE_SUBSCRIBERS = \"{}\"\n",
                p.large_count
            ));
            // The small BLOCK moves with the count, and must: the shim routes
            // on `min(threshold, SUBSCRIBER_BUFFER_SIZE)` (issue 0841), so a
            // count derived against the 2048 split is only sound while the
            // block is sized to hold what that split called small.
            if p.small_max > 0 {
                s.push_str(&format!(
                    "NROS_SUBSCRIBER_BUFFER_SIZE = \"{}\"\n",
                    p.small_max
                ));
            }
            if p.large_count > 0 && p.large_max > 0 {
                s.push_str(&format!(
                    "ZPICO_SUBSCRIBER_LARGE_SIZE = \"{}\"\n",
                    p.large_max
                ));
            }
        }
        crate::leaf_payload_classes::PayloadClasses::Refused { reason } => {
            s.push_str("\n# Payload classes NOT derived (issue 1125); the crate defaults, which\n");
            s.push_str("# are LARGE rather than wrong, stand. A pool short of what this leaf\n");
            s.push_str("# receives is a SubscriberCreationFailed, not a smaller pool.\n");
            for line in reason.lines() {
                s.push_str(&format!("#   {line}\n"));
            }
        }
    }

    // Issue 1233 — the take buffer, over the linked CLOSURE and not over the
    // subscribed set. Appended to the same `[env]` table for the same reason
    // the payload classes are: cargo merges nothing across tables.
    match take {
        crate::leaf_take_buffer::TakeBuffer::Derived(rx) => {
            s.push_str(
                "\n# The runtime take buffer, derived over every type in this leaf's\n\
                 # `generated/` CLOSURE (issue 1233) -- NOT over what it subscribes to.\n\
                 # `RX_BUF` is one global size for every entity and aliases DEFAULT_TX_BUF,\n\
                 # so a type this leaf only PUBLISHES still has to fit. It also feeds\n\
                 # `nros-node`'s arena model, so the arena moves with it.\n",
            );
            s.push_str(&format!("NROS_SUBSCRIPTION_BUFFER_SIZE = \"{rx}\"\n"));
        }
        crate::leaf_take_buffer::TakeBuffer::Refused { reason } => {
            s.push_str(
                "\n# The take buffer is NOT derived (issue 1233); the crate default stands.\n\
                 # Refusing keeps the size this leaf already had -- a maximum computed\n\
                 # from a partial closure can be SMALLER than a type the image sends,\n\
                 # which is a runtime `BufferTooSmall` rather than a build error.\n",
            );
            for line in reason.lines() {
                s.push_str(&format!("#   {line}\n"));
            }
        }
    }
    s
}

/// Which runtime service families a leaf's MANIFEST compiles in:
/// `(param_services, lifecycle_services)`.
///
/// A single-package leaf chooses them with cargo features on its own `nros`
/// dependency (`features = ["lifecycle-services"]`), not with `[system]
/// features` — so its model can say `none` about an image that registers five
/// lifecycle queryables at boot. Any feature string in the manifest naming the
/// family counts (`"param-services"`, `"nros/param-services"`), whether in a
/// dependency's `features` or in `[features]`: over-reading one only reserves
/// slots, while missing one is a registration failure at boot.
pub fn manifest_infra(leaf: &Path) -> (bool, bool) {
    let Ok(text) = std::fs::read_to_string(leaf.join("Cargo.toml")) else {
        return (false, false);
    };
    let Ok(doc) = text.parse::<toml::Table>() else {
        return (false, false);
    };
    let mut strings: Vec<String> = Vec::new();
    let push_features = |v: &toml::Value, out: &mut Vec<String>| {
        if let Some(arr) = v.get("features").and_then(|f| f.as_array()) {
            out.extend(arr.iter().filter_map(|s| s.as_str().map(str::to_string)));
        }
    };
    let mut dep_tables: Vec<&toml::Value> = ["dependencies", "build-dependencies"]
        .iter()
        .filter_map(|k| doc.get(*k))
        .collect();
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for t in targets.values() {
            dep_tables.extend(t.get("dependencies"));
        }
    }
    for deps in dep_tables.into_iter().filter_map(|t| t.as_table()) {
        for spec in deps.values() {
            push_features(spec, &mut strings);
        }
    }
    if let Some(features) = doc.get("features").and_then(|f| f.as_table()) {
        for v in features.values().filter_map(|v| v.as_array()) {
            strings.extend(v.iter().filter_map(|s| s.as_str().map(str::to_string)));
        }
    }
    let has = |family: &str| strings.iter().any(|s| s.ends_with(family));
    (has("param-services"), has("lifecycle-services"))
}

/// RFC-0098 D7 / phase-445 W1 — the `NROS_DECLARED_*` facts a cargo LEAF's
/// image carries, from three sources, each for the half it can answer:
///
/// * `model_facts` — [`crate::cmd::entity_facts::facts_from_model`] over the
///   leaf's resolved model: the node count, the infrastructure families its
///   `[system] features` declare, and the application's service servers when
///   the model describes wiring (an authored contract).
/// * `manifest` — [`manifest_infra`], UNIONED into the infrastructure fact,
///   because a leaf picks those families with cargo features. Same union rule
///   as `InfraServices`: either source declaring a family declares it.
/// * `app_service_servers` — the application's service servers counted by the
///   leaf's OWN inventory (probe, or the `[[component]] entities` declaration),
///   used only where the model abstains. The same inventory already sizes
///   every other derived knob of this road, and it is a complete statement of
///   what the leaf's components create; `None` when it is not (nothing probed
///   and nothing declared, or an un-probeable component skipped).
///
/// No count is formed here — the consumer does that.
pub fn leaf_facts(
    model_facts: &BTreeMap<String, String>,
    manifest: (bool, bool),
    app_service_servers: Option<usize>,
) -> BTreeMap<String, String> {
    let mut out = model_facts.clone();
    if !out.contains_key("NROS_DECLARED_SERVICE_SERVERS")
        && let Some(n) = app_service_servers
    {
        out.insert("NROS_DECLARED_SERVICE_SERVERS".into(), n.to_string());
    }
    // The infrastructure fact exists only where a model stated one. With no
    // model it stays ABSENT, which the consumer reads as "both families
    // present" — the large direction.
    if let Some(infra) = out.get("NROS_DECLARED_INFRA_QUERYABLES").cloned() {
        let param = manifest.0 || matches!(infra.as_str(), "param" | "param+lifecycle" | "all");
        let lifecycle =
            manifest.1 || matches!(infra.as_str(), "lifecycle" | "param+lifecycle" | "all");
        let spelled = match (param, lifecycle) {
            (true, true) => "param+lifecycle",
            (true, false) => "param",
            (false, true) => "lifecycle",
            (false, false) => "none",
        };
        out.insert("NROS_DECLARED_INFRA_QUERYABLES".into(), spelled.into());
    }
    out
}

/// The facts of a leaf's resolved model — `None` when `nros sync` has not
/// resolved one (no `system.toml`, or never synced).
///
/// Reads the model sync WROTE rather than resolving one here: the leaf's launch
/// file is synthesised by sync from its `[[component]]` rows (RFC-0098 D3), so
/// sync is the one producer, and a build that finds no model says `nros sync`
/// (RFC-0098 D2) rather than guessing.
pub fn leaf_model_facts(leaf: &Path) -> Option<BTreeMap<String, String>> {
    use nros_orchestration_ir::model_location;
    if !leaf
        .join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
        .is_file()
    {
        return None;
    }
    let rel = model_location::launch_to_model_rel(leaf, None, &[]).ok()?;
    let path = model_location::resolve_model_path(leaf, &rel);
    if !path.is_file() {
        return None;
    }
    match crate::orchestration::model_ingest::load_model(&path) {
        Ok(m) => Some(crate::cmd::entity_facts::facts_from_model(&m)),
        Err(e) => {
            eprintln!(
                "warning: {}: cannot read the resolved model for entity facts ({e}); the \
                 backend keeps its undeclared queryable budget",
                path.display()
            );
            None
        }
    }
}

/// What a leaf's derived `[env]` is: the rendered sidecar (for the per-leaf
/// `.cargo/` road W6 deletes) and every row as a map (for the image's
/// `build/<image>/nros-cargo.toml`).
#[derive(Debug, Default)]
pub struct LeafEnv {
    /// The sidecar body, when the inventory derived a budget.
    pub sidecar: Option<String>,
    /// Every `[env]` row: the derived pools and the facts.
    pub env: BTreeMap<String, String>,
}

/// Issue 0827 + phase-445 W1 — a leaf's derived `[env]`, ONE computation for
/// both roads that read it (the sync sidecar and the settings file).
///
/// `who` prefixes the diagnostics (`sync`, `nros build`).
///
/// No sidecar on every path that is not a confident derivation, and the cases
/// are deliberately different from each other:
///
/// * no `metadata/` directory, or no probeable component in it — nothing ran, so
///   there is nothing to say;
/// * the inventory REFUSES (`Derivation::Refused`) — it says why, and a refusal
///   is not a budget;
/// * a parse failure — reported, and then treated as "no sidecar", because a
///   budget derived from a half-read probe can be SHORT, and short halts the
///   board. The leaf keeps the crate defaults, which are large rather than wrong.
///
/// The facts are returned in `env` even then: they come from the model and the
/// manifest, and stand without an inventory.
pub fn leaf_env(leaf: &Path, who: &str) -> LeafEnv {
    use crate::entity_inventory::Derivation;

    let model_facts = leaf_model_facts(leaf).unwrap_or_default();
    let manifest = manifest_infra(leaf);
    let bare_facts = || leaf_facts(&model_facts, manifest, None);

    let (inv, unprobeable) = match inventory_for_leaf(leaf) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "{who}: {}: cannot derive pool budgets ({e}); leaving the crate defaults in place",
                leaf.display()
            );
            return LeafEnv {
                sidecar: None,
                env: bare_facts(),
            };
        }
    };
    if inv.is_empty() {
        // An unprobeable component is why a leaf can have `metadata/` and still
        // get no budget. Say so once rather than leaving it to be inferred from
        // an absent file (issue 1061).
        if !unprobeable.is_empty() {
            eprintln!(
                "{who}: {}: {} component(s) are un-probeable, so pool budgets stay at the crate \
                 defaults (issue 1061): {}",
                leaf.display(),
                unprobeable.len(),
                unprobeable.join(", ")
            );
        }
        return LeafEnv {
            sidecar: None,
            env: bare_facts(),
        };
    }
    let declared = matches!(declared_entities(leaf), Ok(Some(_)));
    match inv.derive() {
        Derivation::Derived(knobs) => {
            // The application's servers, as the inventory counts them. Only when
            // the inventory is the WHOLE image: a skipped un-probeable component
            // with no declaration standing in would make this a count of half.
            let app = (unprobeable.is_empty() || declared)
                .then(|| knobs.max_queryables.saturating_sub(knobs.infra_queryables));
            let facts = leaf_facts(&model_facts, manifest, app);
            // Issue 1125 — the payload classes need a SECOND inventory (the
            // per-type bounds `nros sync` has already written into
            // `generated/`), so they are computed here and refuse
            // independently: an entity budget can derive while a subscribed
            // type carries no bound, and the reverse cannot happen.
            let payload = crate::leaf_payload_classes::payload_classes_for_leaf(leaf, &inv);
            if let crate::leaf_payload_classes::PayloadClasses::Refused { reason } = &payload {
                eprintln!(
                    "{who}: {}: payload classes not derived, so `LARGE_PAYLOADS` keeps the \
                     crate default (issue 1125): {reason}",
                    leaf.display()
                );
            }
            // Issue 1233 — a THIRD basis: the take buffer is derived over the
            // leaf's whole `generated/` closure, so it refuses on inputs the
            // payload join accepts and accepts inputs it refuses (a leaf that
            // subscribes to nothing still links types it publishes).
            let take = crate::leaf_take_buffer::take_buffer_for_leaf(leaf);
            if let crate::leaf_take_buffer::TakeBuffer::Refused { reason } = &take {
                eprintln!(
                    "{who}: {}: take buffer not derived, so `RX_BUF` keeps the crate \
                     default (issue 1233): {reason}",
                    leaf.display()
                );
            }
            let body = render_env_sidecar_with_facts(&knobs, &payload, &take, &inv.source, &facts);
            let env = env_rows(&body);
            LeafEnv {
                sidecar: Some(body),
                env,
            }
        }
        other => {
            eprintln!(
                "{who}: {}: pool budgets not derived ({}); crate defaults stay",
                leaf.display(),
                other.tag()
            );
            LeafEnv {
                sidecar: None,
                env: bare_facts(),
            }
        }
    }
}

/// The string rows of a rendered sidecar's `[env]`, read back rather than
/// re-spelled, so the renderer stays the one place the knob names live.
pub fn env_rows(body: &str) -> BTreeMap<String, String> {
    body.parse::<toml::Value>()
        .ok()
        .and_then(|v| v.get("env").and_then(|e| e.as_table()).cloned())
        .map(|t| {
            t.into_iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_inventory::Derivation;

    /// The take buffer these tests are not about. A REFUSAL rather than a
    /// derived number: it leaves the rendered `[env]` with no
    /// `NROS_SUBSCRIPTION_BUFFER_SIZE` row, so a test asserting on the budget
    /// keys sees exactly what it saw before issue 1233.
    fn refused_take() -> crate::leaf_take_buffer::TakeBuffer {
        crate::leaf_take_buffer::TakeBuffer::Refused {
            reason: "not under test".into(),
        }
    }

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

    // ---- phase-445 W1: the facts on the cargo-leaf road -------------------

    fn facts(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// The esp32 talker's case: the model abstains on the application count
    /// (no contract), the inventory knows it is zero, and nothing enables a
    /// service family. The consumer then sizes `max(0 + 0, 1)` = 1 slot.
    #[test]
    fn the_inventory_answers_the_application_count_the_model_abstains_on() {
        let model = facts(&[
            ("NROS_DECLARED_INFRA_QUERYABLES", "none"),
            ("NROS_DECLARED_NODES", "1"),
        ]);
        let out = leaf_facts(&model, (false, false), Some(0));
        assert_eq!(out["NROS_DECLARED_SERVICE_SERVERS"], "0");
        assert_eq!(out["NROS_DECLARED_INFRA_QUERYABLES"], "none");
        assert_eq!(out["NROS_DECLARED_NODES"], "1");
    }

    /// A contract's count is the model's to state; the inventory never
    /// overrides a stated fact.
    #[test]
    fn a_model_stated_application_count_wins() {
        let model = facts(&[
            ("NROS_DECLARED_SERVICE_SERVERS", "3"),
            ("NROS_DECLARED_INFRA_QUERYABLES", "none"),
        ]);
        let out = leaf_facts(&model, (false, false), Some(0));
        assert_eq!(out["NROS_DECLARED_SERVICE_SERVERS"], "3");
    }

    /// The safety property. A leaf picks the service families with CARGO
    /// features, so a model saying `none` about an image whose manifest enables
    /// `lifecycle-services` would size a table five short — a registration
    /// failure at boot. Either source declaring a family declares it.
    #[test]
    fn a_manifest_enabled_service_family_is_unioned_into_the_model_fact() {
        let model = facts(&[("NROS_DECLARED_INFRA_QUERYABLES", "none")]);
        assert_eq!(
            leaf_facts(&model, (false, true), Some(0))["NROS_DECLARED_INFRA_QUERYABLES"],
            "lifecycle"
        );
        assert_eq!(
            leaf_facts(&model, (true, true), Some(0))["NROS_DECLARED_INFRA_QUERYABLES"],
            "param+lifecycle"
        );
        let model = facts(&[("NROS_DECLARED_INFRA_QUERYABLES", "param")]);
        assert_eq!(
            leaf_facts(&model, (false, true), None)["NROS_DECLARED_INFRA_QUERYABLES"],
            "param+lifecycle"
        );
    }

    /// No model, no infrastructure fact: ABSENT is what the consumer reads as
    /// "both families present", the large direction. Inventing `none` here
    /// would be the short one.
    #[test]
    fn no_model_states_no_infrastructure_fact() {
        let out = leaf_facts(&BTreeMap::new(), (false, false), Some(2));
        assert!(
            !out.contains_key("NROS_DECLARED_INFRA_QUERYABLES"),
            "{out:?}"
        );
        assert_eq!(out["NROS_DECLARED_SERVICE_SERVERS"], "2");
    }

    #[test]
    fn manifest_infra_reads_dependency_and_feature_tables() {
        let td = tempfile::tempdir().unwrap();
        write_manifest(
            td.path(),
            r#"
[package]
name = "p"
[features]
default = ["lifecycle"]
lifecycle = ["nros/lifecycle-services"]
[dependencies]
nros = { version = "*", features = ["std", "param-services"] }
"#,
        );
        assert_eq!(manifest_infra(td.path()), (true, true));
        write_manifest(
            td.path(),
            "[package]\nname = \"p\"\n# param-services in a comment is not a feature\n\
             [dependencies]\nnros = { version = \"*\", features = [\"std\"] }\n",
        );
        assert_eq!(manifest_infra(td.path()), (false, false));
    }

    /// With facts, the sidecar says where `ZPICO_MAX_QUERYABLES` comes from and
    /// still never states it as a count (issue 0460).
    #[test]
    fn the_sidecar_carries_the_facts_and_still_no_queryable_count() {
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Derived(Default::default());
        let f = facts(&[
            ("NROS_DECLARED_INFRA_QUERYABLES", "none"),
            ("NROS_DECLARED_NODES", "1"),
            ("NROS_DECLARED_SERVICE_SERVERS", "0"),
        ]);
        let out = render_env_sidecar_with_facts(&k, &payload, &refused_take(), "t", &f);
        let rows = env_rows(&out);
        assert_eq!(rows["NROS_DECLARED_SERVICE_SERVERS"], "0");
        assert_eq!(rows["NROS_DECLARED_INFRA_QUERYABLES"], "none");
        assert!(!rows.contains_key("ZPICO_MAX_QUERYABLES"), "{out}");
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
        // The DEMAND, unfloored: a talker declares no subscription, so it
        // demands none. Issue 1015 floored this in the shared derivation and
        // that reached the XRCE pools too, where a slot costs 33,296 bytes of
        // heap and zero is the measured answer (issue 1033). The floor now
        // lives at the `ZPICO_*` rows of the sidecar — see
        // `the_zpico_rows_are_floored_and_the_others_are_not`.
        assert_eq!(
            k.max_subscribers, 0,
            "the demand is the count, not a pool size"
        );
        assert_eq!(
            k.max_cbs, 1,
            "the timer claims the slot; the publisher does not"
        );
    }

    /// Issue 1015 + issue 1033 — the floor is the CONSUMER's, so it applies to
    /// the two knobs that size C arrays and to nothing else.
    ///
    /// A talker demands zero subscriptions. `ZPICO_MAX_SUBSCRIBERS` sizes
    /// `subscriber_entry_t subscribers[...]` in `zpico.c` and must still be 1;
    /// `NROS_RMW_SUBSCRIBER_SLOTS` sizes a Rust array, where zero is a legal
    /// pool that fails loudly at registration, and must carry the raw 0.
    #[test]
    fn the_zpico_rows_are_floored_and_the_others_are_not() {
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Derived(Default::default());
        let out = render_env_sidecar(&k, &payload, &refused_take(), "test");
        // Whole rows, matched as text through `contains`, NOT through a local
        // `row(NAME)` helper: `config-knob-census` reads this file as a
        // build-time knob source and refuses an unknown callee taking a knob
        // name, which is how a `knob()` wrapper once took five knobs out of
        // that census. Measured — the helper version failed the gate.
        assert!(
            out.contains("ZPICO_MAX_SUBSCRIBERS = \"1\"\n"),
            "floored at one (issue 1015), not the raw 0:\n{out}"
        );
        assert!(
            out.contains("ZPICO_MAX_PUBLISHERS = \"1\"\n"),
            "the talker's one publisher:\n{out}"
        );
        assert!(
            out.contains("NROS_RMW_SUBSCRIBER_SLOTS = \"0\"\n"),
            "a Rust-backed pool takes the demand; only the C arrays are floored:\n{out}"
        );
    }

    // ---- issue 1061: the leaf's declaration ----------------------------
    //
    // Stated on the leaf's `system.toml` `[[component]]` rows (RFC-0098 D8).
    // The manifest spelling (`[package.metadata.nros.component] entities`) was
    // read through a fallback phase-445 W5 deleted.

    /// A leaf whose `system.toml` declares one component with `entities`
    /// (`None` = the key absent).
    /// Just the manifest, with the caller's own body.
    ///
    /// Sibling of [`write_leaf`] and deliberately NOT a parameter on it: that
    /// one writes a FIXED manifest plus the `system.toml` whose `entities` the
    /// entity tests vary, while `manifest_infra` reads the manifest's own
    /// `[features]` / `[dependencies]` and needs no `system.toml` at all.
    /// Folding the two would make every entity test state a manifest it does
    /// not care about.
    fn write_manifest(dir: &std::path::Path, body: &str) {
        std::fs::write(dir.join("Cargo.toml"), body).unwrap();
    }

    fn write_leaf(dir: &std::path::Path, entities: Option<&str>) {
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"p\"\n").unwrap();
        let decl = entities
            .map(|e| format!("entities = {e}\n"))
            .unwrap_or_default();
        std::fs::write(
            dir.join("system.toml"),
            format!(
                "[system]\nname = \"p\"\nrmw = \"zenoh\"\n\n[[component]]\npkg = \"p\"\n{decl}\n\
                 [image.native]\nboard = \"native\"\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn a_manifest_declaration_parses_with_the_shared_grammar() {
        let td = tempfile::tempdir().unwrap();
        write_leaf(
            td.path(),
            Some(r#"["publisher:std_msgs/msg/String:/chatter", "timer", "sub*2"]"#),
        );
        let d = declared_entities(td.path()).unwrap().expect("declared");
        // `sub*2` is the repeat suffix -- the SHARED grammar's, not a second one.
        assert_eq!(d.len(), 4, "{d:?}");
        assert_eq!(d[0].kind, EntityKind::Publisher);
        assert_eq!(d[0].name.as_deref(), Some("/chatter"));
        assert_eq!(d[1].kind, EntityKind::Timer);
        assert_eq!(d[2].kind, EntityKind::Subscription);
        assert_eq!(d[3].kind, EntityKind::Subscription);
    }

    /// Absent and empty are DIFFERENT: absent means the leaf said nothing, empty
    /// means it asserted it creates nothing.
    #[test]
    fn an_absent_key_is_none_and_an_empty_list_is_some_empty() {
        let td = tempfile::tempdir().unwrap();
        write_leaf(td.path(), None);
        assert!(declared_entities(td.path()).unwrap().is_none());

        write_leaf(td.path(), Some("[]"));
        assert_eq!(declared_entities(td.path()).unwrap(), Some(vec![]));
    }

    #[test]
    fn a_malformed_declaration_names_the_entry() {
        let td = tempfile::tempdir().unwrap();
        write_leaf(td.path(), Some("[\"nonsense\"]"));
        let e = declared_entities(td.path()).unwrap_err().to_string();
        assert!(e.contains("nonsense"), "{e}");

        write_leaf(td.path(), Some("\"timer\""));
        let e = declared_entities(td.path()).unwrap_err().to_string();
        assert!(e.contains("ARRAY"), "{e}");
    }

    /// The safety property: a declaration that disagrees with the code REFUSES.
    /// Without this the manifest becomes a way to state a budget smaller than
    /// the image, and short halts the board.
    #[test]
    fn a_declaration_that_disagrees_with_the_probe_refuses() {
        let probed = EntityDecl::parse("publisher").unwrap();
        let same = EntityDecl::parse("publisher").unwrap();
        assert!(reconcile("c", &same, &probed).is_ok());

        let fewer = EntityDecl::parse("timer").unwrap();
        let e = reconcile("c", &fewer, &probed).unwrap_err().to_string();
        assert!(e.contains("declares") && e.contains("creates"), "{e}");
        assert!(e.contains("timerx1") && e.contains("publisherx1"), "{e}");
    }

    /// Compared by KIND COUNT, not by row: a declaration may legitimately state
    /// a topic loosely, and the budget is computed from counts.
    #[test]
    fn reconcile_compares_counts_not_topic_names() {
        let probed = EntityDecl::parse("sub:std_msgs/msg/String:/chatter").unwrap();
        let declared = EntityDecl::parse("sub").unwrap();
        assert!(reconcile("c", &declared, &probed).is_ok());
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Derived(Default::default());
        let out = render_env_sidecar(&k, &payload, &refused_take(), "metadata/talker.json");
        assert!(out.contains("[env]"));
        for key in DERIVED_ENV_KEYS {
            assert!(out.contains(key), "sidecar omits {key}:\n{out}");
        }
        assert!(out.contains("ZPICO_MAX_SUBSCRIBERS = \"1\""), "{out}");
        assert!(out.contains("ZPICO_MAX_PUBLISHERS = \"1\""), "{out}");
        // The infra-incomplete knob must NOT be stated as a value.
        assert!(
            !out.lines().any(|l| l.starts_with("ZPICO_MAX_QUERYABLES")),
            "the sidecar states a queryable count it cannot complete:\n{out}"
        );
        // phase-412 W2 -- and the liveliness count, for the same reason.
        assert!(
            !out.lines().any(|l| l.starts_with("ZPICO_MAX_LIVELINESS")),
            "the sidecar states a liveliness count it cannot complete:\n{out}"
        );
        // Issue 1130 -- the cell capacity IS stated: this leaf's one component
        // declares one publisher, so its registries need one slot per kind.
        assert!(
            out.contains("NROS_RUNTIME_MAX_CELL_ENTITIES = \"1\""),
            "{out}"
        );
        // No `force = true` KEY: a value the caller states must win. Checked
        // line-wise, because the header prose explains `force` and a substring
        // test would match the explanation rather than a setting.
        assert!(
            !out.lines().any(|l| l.trim_start().starts_with("force")),
            "sidecar sets `force`, so a caller's own value would be overridden:\n{out}"
        );
    }

    /// Issue 1125 — the publisher-only leaf that opened the issue. The count is
    /// stated as ZERO, which is the whole 131,072 B saving; the large SIZE is
    /// not stated at all, because a class with no blocks has no size worth
    /// naming.
    #[test]
    fn a_leaf_with_no_subscription_states_a_zero_large_count_and_no_large_size() {
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Derived(Default::default());
        let out = render_env_sidecar(&k, &payload, &refused_take(), "metadata/talker.json");
        assert!(
            out.contains("ZPICO_MAX_LARGE_SUBSCRIBERS = \"0\""),
            "the derived zero must be STATED, not left to the crate default:\n{out}"
        );
        for k in ["ZPICO_SUBSCRIBER_LARGE_SIZE", "NROS_SUBSCRIBER_BUFFER_SIZE"] {
            assert!(
                !out.lines().any(|l| l.starts_with(k)),
                "sidecar names {k} for a class it just declared empty:\n{out}"
            );
        }
    }

    /// A leaf that DOES subscribe large states all three, and the small block
    /// is stated too — the count is only sound while the block holds what the
    /// split called small (issue 0841).
    #[test]
    fn a_large_subscription_states_the_count_the_size_and_the_small_block() {
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Derived(
            crate::leaf_payload_classes::DerivedPayloadClasses {
                large_count: 2,
                large_max: 40_000,
                small_max: 1500,
                subscribed: 3,
            },
        );
        let out = render_env_sidecar(&k, &payload, &refused_take(), "metadata/talker.json");
        assert!(out.contains("ZPICO_MAX_LARGE_SUBSCRIBERS = \"2\""), "{out}");
        assert!(
            out.contains("ZPICO_SUBSCRIBER_LARGE_SIZE = \"40000\""),
            "{out}"
        );
        assert!(
            out.contains("NROS_SUBSCRIBER_BUFFER_SIZE = \"1500\""),
            "{out}"
        );
    }

    /// A refusal states NO payload key at all — it explains itself in a comment
    /// and leaves the crate defaults, which are large rather than wrong.
    #[test]
    fn a_refused_payload_join_states_no_payload_key() {
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
        let payload = crate::leaf_payload_classes::PayloadClasses::Refused {
            reason: "std_msgs/msg/String (unbounded)".into(),
        };
        let out = render_env_sidecar(&k, &payload, &refused_take(), "metadata/talker.json");
        for key in DERIVED_PAYLOAD_ENV_KEYS {
            assert!(
                !out.lines().any(|l| l.starts_with(key)),
                "a refused join states {key}:\n{out}"
            );
        }
        assert!(out.contains("# Payload classes NOT derived"), "{out}");
        // The entity half still lands — the two inventories refuse
        // independently, which is why they are two lists.
        assert!(out.contains("ZPICO_MAX_SUBSCRIBERS = \"1\""), "{out}");
    }
}
