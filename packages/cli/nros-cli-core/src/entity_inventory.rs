//! phase-403 W9 (issue 0965) -- WHICH ENTITIES AN IMAGE CREATES.
//!
//! The bound inventory (`rosidl_codegen::bounds`) prices a TYPE. It cannot say
//! whether an image subscribes to that type, and three consumers need the
//! second question answered: the zenoh payload class boundaries, the executor
//! arena, and `NROS_EXECUTOR_MAX_CBS`. This module is the second source.
//!
//! It follows `bounds.rs`'s shape deliberately -- ONE data model, rendered into
//! the transports the later stages already speak -- rather than inventing a
//! second inventory mechanism:
//!
//! * [`EntityInventory::to_json`] -- canonical, `nros_entity_inventory.json`.
//! * [`EntityInventory::to_cmake`] -- an `include()`able fragment for the
//!   CMake/Kconfig lane, the same projection `nros_message_bounds.cmake` is.
//! * [`EntityInventory::to_env`] -- `KEY=VALUE` lines, the carrier that reaches
//!   a cargo invocation. `bounds.rs` uses a generated crate's `links` key for
//!   this rung; an entity inventory has no generated crate of its own, and the
//!   knob it feeds (`NROS_EXECUTOR_MAX_CBS`) is read from the ENVIRONMENT by
//!   `nros-node/build.rs`. So the env line IS the cargo transport here, and it
//!   is the same one `nros ws entity-facts` already publishes through
//!   `corrosion_set_env_vars` (`cmake/NanoRosEntityFacts.cmake`).
//!
//! # Where the declaration comes from, and why it is AUTHOR-STATED
//!
//! RFC-0043/0044 components create their entities in CONSTRUCTORS, at runtime.
//! The registration macros (`NROS_SUBSCRIBE`, `create_publisher`,
//! `NROS_CREATE_TIMER`) do know the kind and the type `M` -- but anything they
//! emit is a LINK-SECTION fact, and it exists only after linking. The numbers
//! this inventory feeds are `const` sizes compiled INTO `nros-node`, which is
//! built before a single component TU is compiled. A link-section manifest can
//! therefore VERIFY a count and can never SUPPLY one; that is the direction of
//! the build graph, not a gap in the tooling.
//!
//! So the declaration is stated where the component is already declared --
//! `nano_ros_node_register(... ENTITIES ...)`, beside `CLASS`, `SHAPE` and
//! `CALLBACK_GROUPS` -- and travels the channel that declaration already
//! travels, `nros-metadata.json`.
//!
//! # An under-report can never be silent
//!
//! Three layers, in the order they fire:
//!
//! 1. **Composition refuses on INCOMPLETE data.** If any component in the image
//!    states no `ENTITIES` at all, [`EntityInventory::derive`] refuses for the
//!    WHOLE image and no knob is derived -- the same rule
//!    `NanoRosMessageBounds.cmake` holds when any type in the closure is
//!    unbounded. A component that really creates nothing says so explicitly
//!    (`ENTITIES NONE`), so ABSENCE always means "nobody said", never "zero".
//! 2. **The derived value carries NO headroom.** It is exactly the declared
//!    slot demand. That is deliberate: it makes the running image a CHECKER of
//!    its own manifest.
//! 3. **A short manifest is a named boot failure.** Registration past the table
//!    returns `NodeError::ExecutorFull`, which names the knob, and
//!    `ComponentNode`'s `ok()` flag makes the entry halt boot naming the
//!    failing node. `MAX_CBS` is the right FIRST consumer precisely because its
//!    under-size failure is already loud: an under-sized ARENA halts during
//!    entity creation, before the first spin, which is why issue 0900 W1's
//!    advisory cannot cover it.
//!
//! # A publisher claims no callback slot, and that is MEASURED
//!
//! `NROS_EXECUTOR_MAX_CBS` sizes the executor's callback-entry table. Every
//! registration site that claims one calls `Executor::next_entry_slot()`, and
//! the 24 sites that do are subscriptions, timers, services, service clients,
//! action servers, action clients and guard conditions. `create_publisher` is
//! not among them -- on the C++ path it writes an `RmwPublisher` into
//! caller-owned storage (`nros-cpp/src/publisher.rs`) and on the C path there
//! is no `nros_executor_add_publisher` to increment `handle_count`.
//!
//! This matters because the mr-canhubk344 bring-up recorded "33 handles" for
//! the island and set `MAX_CBS=36` from it. 33 is the ENTITY count; 14 of those
//! are publishers, which claim no slot. Both numbers are in the inventory, and
//! [`EntityKind::callback_slots`] is the only place the difference is spelled.
//!
//! # The infrastructure services are NOT a hidden term, and that was checked
//!
//! The obvious way for this derivation to be short is an entity the executor
//! creates that no component declares. There are two candidates and neither
//! claims a slot: `ParamState` is "stored outside the arena so it doesn't
//! consume `MAX_CBS` slots" (`parameter_services.rs`), and the five REP-2002
//! lifecycle servers go through `create_lc_srv`, which calls
//! `session.create_service` directly and never
//! `Executor::register_service_*`. So the declared application entities are the
//! whole demand -- which is what makes rule 2 above (no headroom) a checkable
//! claim rather than a hopeful one.

use std::collections::BTreeMap;

/// Bumped when the shape of the emitted inventory changes incompatibly.
/// A consumer that does not recognise the version must refuse, never guess.
pub const ENTITY_INVENTORY_SCHEMA_VERSION: u32 = 1;

/// Canonical artifact name.
pub const ENTITY_INVENTORY_JSON_NAME: &str = "nros_entity_inventory.json";

/// CMake projection of [`ENTITY_INVENTORY_JSON_NAME`], beside it.
pub const ENTITY_INVENTORY_CMAKE_NAME: &str = "nros_entity_inventory.cmake";

/// A kind of entity a component creates.
///
/// The set is closed on purpose: an unrecognised spelling is a REFUSAL, never a
/// row this module skips. A skipped row is exactly an under-report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntityKind {
    Publisher,
    Subscription,
    Timer,
    ServiceServer,
    ServiceClient,
    ActionServer,
    ActionClient,
    GuardCondition,
}

/// Every kind, in emission order. The one list; a second one is how a kind
/// silently stops being counted.
pub const ALL_ENTITY_KINDS: &[EntityKind] = &[
    EntityKind::Publisher,
    EntityKind::Subscription,
    EntityKind::Timer,
    EntityKind::ServiceServer,
    EntityKind::ServiceClient,
    EntityKind::ActionServer,
    EntityKind::ActionClient,
    EntityKind::GuardCondition,
];

impl EntityKind {
    /// The canonical spelling, used on every transport and in the declaration.
    pub fn tag(self) -> &'static str {
        match self {
            EntityKind::Publisher => "publisher",
            EntityKind::Subscription => "subscription",
            EntityKind::Timer => "timer",
            EntityKind::ServiceServer => "service_server",
            EntityKind::ServiceClient => "service_client",
            EntityKind::ActionServer => "action_server",
            EntityKind::ActionClient => "action_client",
            EntityKind::GuardCondition => "guard_condition",
        }
    }

    /// How many `NROS_EXECUTOR_MAX_CBS` callback-entry slots ONE entity of this
    /// kind claims.
    ///
    /// MIRROR of the `Executor::next_entry_slot()` call sites in
    /// `packages/core/nros-node/src/executor/{spin,action}.rs`, held to them by
    /// `scripts/check-entity-slot-costs.py`. The CLI cannot depend on
    /// `nros-node` -- that crate is `no_std`, platform-gated and built for the
    /// target, not the host -- so the mapping is restated here AND gated, which
    /// is the difference between this and a comment that drifts.
    ///
    /// A publisher is 0. See the module docs: it is the number the island's
    /// hand-count got wrong, and it is worth 14 slots on that image.
    pub fn callback_slots(self) -> usize {
        match self {
            EntityKind::Publisher => 0,
            EntityKind::Subscription
            | EntityKind::Timer
            | EntityKind::ServiceServer
            | EntityKind::ServiceClient
            | EntityKind::ActionServer
            | EntityKind::ActionClient
            | EntityKind::GuardCondition => 1,
        }
    }

    /// Parse one declared kind.
    ///
    /// Accepts the canonical [`Self::tag`] plus the short spellings a human
    /// writing a CMake argument reaches for. Anything else is an ERROR and not
    /// a skipped row -- see the type docs.
    pub fn parse(s: &str) -> Result<Self, String> {
        let norm = s.trim().to_ascii_lowercase().replace('-', "_");
        Ok(match norm.as_str() {
            "publisher" | "pub" => EntityKind::Publisher,
            "subscription" | "sub" | "subscriber" => EntityKind::Subscription,
            "timer" | "tmr" => EntityKind::Timer,
            "service_server" | "service" | "srv" | "server" => EntityKind::ServiceServer,
            "service_client" | "client" => EntityKind::ServiceClient,
            "action_server" => EntityKind::ActionServer,
            "action_client" => EntityKind::ActionClient,
            "guard_condition" | "guard" => EntityKind::GuardCondition,
            _ => {
                return Err(format!(
                    "unknown entity kind `{s}` -- expected one of: {}",
                    ALL_ENTITY_KINDS
                        .iter()
                        .map(|k| k.tag())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        })
    }
}

/// One declared entity.
///
/// `type_name` is the ROS name the bound inventory prices (`pkg/msg/Name`) --
/// the same spelling `TypeBoundEntry::type_name` uses, so the two inventories
/// join without a second naming convention. It is OPTIONAL because a timer and
/// a guard condition carry no type, and because a count is useful before every
/// call site has been annotated. `name` is the topic / service / action name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityDecl {
    pub kind: EntityKind,
    pub type_name: Option<String>,
    pub name: Option<String>,
}

impl EntityDecl {
    /// Parse the declaration spelling: `<kind>[:<type>[:<name>]]`.
    ///
    /// `sub:nav_msgs/msg/Odometry:/localization/kinematic_state`, `timer`,
    /// `publisher:autoware_vehicle_msgs/msg/GearCommand`.
    ///
    /// A `*N` suffix on the kind repeats it: `timer*3`. A repeat count is the
    /// one concession to brevity, and it is on the KIND rather than a separate
    /// argument so a row can never lose its multiplier in transit.
    pub fn parse(spec: &str) -> Result<Vec<Self>, String> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("empty entity declaration".to_string());
        }
        let mut parts = spec.splitn(3, ':');
        let kind_field = parts.next().unwrap_or("");
        let type_name = parts.next().map(str::trim).filter(|s| !s.is_empty());
        let name = parts.next().map(str::trim).filter(|s| !s.is_empty());

        let (kind_str, repeat) = match kind_field.split_once('*') {
            Some((k, n)) => {
                let n: usize = n.trim().parse().map_err(|_| {
                    format!("entity declaration `{spec}`: `{n}` is not a repeat count")
                })?;
                if n == 0 {
                    return Err(format!(
                        "entity declaration `{spec}`: a repeat count of 0 states nothing. \
                         Omit the row, or declare the component `NONE`."
                    ));
                }
                (k, n)
            }
            None => (kind_field, 1),
        };
        let kind = EntityKind::parse(kind_str).map_err(|e| format!("in `{spec}`: {e}"))?;
        Ok((0..repeat)
            .map(|_| EntityDecl {
                kind,
                type_name: type_name.map(str::to_string),
                name: name.map(str::to_string),
            })
            .collect())
    }
}

/// What one component said about its entities.
///
/// Three-valued for the reason [`rosidl_codegen::bounds::BoundState`] is:
/// "it creates none" and "it did not say" license completely different actions,
/// and collapsing them is exactly the under-report this module exists to make
/// impossible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaration {
    /// `ENTITIES <spec>...` -- the component named what it creates.
    Stated(Vec<EntityDecl>),
    /// `ENTITIES NONE` -- the component asserts it creates nothing.
    None,
    /// The register call carried no `ENTITIES` at all.
    Absent,
}

impl Declaration {
    pub fn tag(&self) -> &'static str {
        match self {
            Declaration::Stated(_) => "stated",
            Declaration::None => "none",
            Declaration::Absent => "absent",
        }
    }

    /// The declared entities; empty for both `None` and `Absent`. Callers must
    /// distinguish those two through [`Declaration::tag`], never through the
    /// length of this slice.
    pub fn entities(&self) -> &[EntityDecl] {
        match self {
            Declaration::Stated(v) => v,
            Declaration::None | Declaration::Absent => &[],
        }
    }
}

/// One component's row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentEntities {
    /// ament package the component lives in.
    pub pkg: String,
    /// The `NAME` the register call gave it -- the launch `exec`.
    pub component: String,
    /// The qualified C++ class, so a refusal names something a user can grep.
    pub class: String,
    pub declaration: Declaration,
}

/// Every component in ONE image, with what each declared.
///
/// The unit is the IMAGE, not the package: `MAX_CBS` sizes one executor and an
/// image has one. That is the same reason `nros_derive_message_bound_knobs`
/// composes over the whole linked closure rather than per package.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityInventory {
    /// Where this inventory came from, for the provenance line. Usually the
    /// `nros-metadata.json` path.
    pub source: String,
    components: Vec<ComponentEntities>,
}

/// The knobs an entity inventory can answer, plus how it got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedEntityKnobs {
    /// `NROS_EXECUTOR_MAX_CBS` -- the total callback-entry slot demand.
    pub max_cbs: usize,
    /// Every declared entity, slot-claiming or not. NOT the knob: kept because
    /// it is the number a human counts, and because the gap between the two is
    /// the finding.
    pub entity_total: usize,
    /// Per-kind counts across the image, in [`ALL_ENTITY_KINDS`] order.
    pub per_kind: BTreeMap<&'static str, usize>,
    /// Per-component `(pkg, component, entities, slots)`, so the output records
    /// which declaration contributed what.
    pub per_component: Vec<(String, String, usize, usize)>,
}

/// The result of composing an image's declarations.
///
/// `Refused` carries prose and NO number: a consumer either reads a value this
/// module derived or reads nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derivation {
    Derived(Box<DerivedEntityKnobs>),
    Refused { reason: String },
}

impl Derivation {
    pub fn tag(&self) -> &'static str {
        match self {
            Derivation::Derived(_) => "derived",
            Derivation::Refused { .. } => "refused",
        }
    }

    pub fn knobs(&self) -> Option<&DerivedEntityKnobs> {
        match self {
            Derivation::Derived(k) => Some(k),
            Derivation::Refused { .. } => None,
        }
    }
}

impl EntityInventory {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            components: Vec::new(),
        }
    }

    /// Record one component. A later record for the same `(pkg, component)`
    /// replaces the earlier one, so a configure that registers a component
    /// twice cannot double-count it.
    pub fn insert(&mut self, row: ComponentEntities) {
        match self
            .components
            .iter_mut()
            .find(|c| c.pkg == row.pkg && c.component == row.component)
        {
            Some(existing) => *existing = row,
            None => self.components.push(row),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Rows in emission order: sorted by `(pkg, component)`, so the artifact is
    /// byte-stable across runs and a write-if-changed keeps mtimes still.
    pub fn components(&self) -> Vec<&ComponentEntities> {
        let mut v: Vec<&ComponentEntities> = self.components.iter().collect();
        v.sort_by(|a, b| (&a.pkg, &a.component).cmp(&(&b.pkg, &b.component)));
        v
    }

    /// Compose the image's declarations into the knobs, or REFUSE.
    ///
    /// Refuses when the image has no components at all, and when ANY component
    /// stated nothing. Partial data never yields a number: an image whose
    /// fourth node has not been annotated would otherwise derive a total three
    /// nodes' worth short, and a short `MAX_CBS` is a failed entity creation on
    /// a board.
    pub fn derive(&self) -> Derivation {
        if self.components.is_empty() {
            return Derivation::Refused {
                reason: "no components were registered in this image, so there is nothing to \
                         compose. `nano_ros_node_register()` is what puts a component here."
                    .to_string(),
            };
        }

        let undeclared: Vec<&ComponentEntities> = self
            .components()
            .into_iter()
            .filter(|c| matches!(c.declaration, Declaration::Absent))
            .collect();
        if !undeclared.is_empty() {
            let block = undeclared
                .iter()
                .map(|c| format!("    {}::{} ({})", c.pkg, c.component, c.class))
                .collect::<Vec<_>>()
                .join("\n");
            return Derivation::Refused {
                reason: format!(
                    "{} of {} components in this image declare no entities:\n{block}\n\
                     Deriving over only the components that did would publish a slot count \
                     smaller than the image needs, and a short NROS_EXECUTOR_MAX_CBS fails \
                     entity creation at boot. Add `ENTITIES ...` to each \
                     `nano_ros_node_register()` above -- `ENTITIES NONE` for a component that \
                     really creates none.",
                    undeclared.len(),
                    self.components.len()
                ),
            };
        }

        let mut per_kind: BTreeMap<&'static str, usize> = BTreeMap::new();
        for k in ALL_ENTITY_KINDS {
            per_kind.insert(k.tag(), 0);
        }
        let mut per_component = Vec::new();
        let mut max_cbs = 0usize;
        let mut entity_total = 0usize;
        for c in self.components() {
            let mut slots = 0usize;
            let mut count = 0usize;
            for e in c.declaration.entities() {
                *per_kind.entry(e.kind.tag()).or_insert(0) += 1;
                slots += e.kind.callback_slots();
                count += 1;
            }
            max_cbs += slots;
            entity_total += count;
            per_component.push((c.pkg.clone(), c.component.clone(), count, slots));
        }

        Derivation::Derived(Box::new(DerivedEntityKnobs {
            max_cbs,
            entity_total,
            per_kind,
            per_component,
        }))
    }

    /// The canonical artifact.
    pub fn to_json(&self) -> String {
        let derivation = self.derive();
        let components: Vec<serde_json::Value> = self
            .components()
            .into_iter()
            .map(|c| {
                let mut m = serde_json::Map::new();
                m.insert("pkg".into(), c.pkg.clone().into());
                m.insert("component".into(), c.component.clone().into());
                m.insert("class".into(), c.class.clone().into());
                m.insert("declaration".into(), c.declaration.tag().into());
                m.insert(
                    "entities".into(),
                    c.declaration
                        .entities()
                        .iter()
                        .map(|e| {
                            let mut r = serde_json::Map::new();
                            r.insert("kind".into(), e.kind.tag().into());
                            r.insert("callback_slots".into(), e.kind.callback_slots().into());
                            if let Some(t) = &e.type_name {
                                r.insert("type_name".into(), t.clone().into());
                            }
                            if let Some(n) = &e.name {
                                r.insert("name".into(), n.clone().into());
                            }
                            serde_json::Value::Object(r)
                        })
                        .collect::<Vec<_>>()
                        .into(),
                );
                serde_json::Value::Object(m)
            })
            .collect();

        let mut doc = serde_json::Map::new();
        doc.insert(
            "schema_version".into(),
            ENTITY_INVENTORY_SCHEMA_VERSION.into(),
        );
        doc.insert("producer".into(), "nros ws entity-inventory".into());
        doc.insert("source".into(), self.source.clone().into());
        doc.insert("status".into(), derivation.tag().into());
        doc.insert("components".into(), components.into());
        match &derivation {
            Derivation::Derived(k) => {
                doc.insert("entity_total".into(), k.entity_total.into());
                doc.insert("max_cbs".into(), k.max_cbs.into());
                doc.insert(
                    "per_kind".into(),
                    serde_json::Value::Object(
                        k.per_kind
                            .iter()
                            .map(|(name, n)| ((*name).to_string(), (*n).into()))
                            .collect(),
                    ),
                );
            }
            Derivation::Refused { reason } => {
                doc.insert("reason".into(), reason.clone().into());
            }
        }
        format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::Value::Object(doc)).unwrap_or_default()
        )
    }

    /// The CMake/Kconfig projection.
    ///
    /// A REFUSAL sets a status and a reason and NO `NROS_DERIVED_*` variable, so
    /// a consumer that reads a number reads one this module derived or reads
    /// nothing -- the rule `nros_message_bounds.cmake` holds for a type with no
    /// bound.
    pub fn to_cmake(&self) -> String {
        let derivation = self.derive();
        let mut s = String::new();
        s.push_str("# GENERATED by `nros ws entity-inventory` (phase-403 W9, issue 0965).\n");
        s.push_str("# Do not edit.\n#\n");
        s.push_str(
            "# WHICH ENTITIES THIS IMAGE CREATES, composed from every\n\
             # `nano_ros_node_register(... ENTITIES ...)` in it. The bound inventory\n\
             # (`nros_message_bounds.cmake`) prices a TYPE; this one counts the entities,\n\
             # which is the half `NROS_EXECUTOR_MAX_CBS` needs.\n#\n",
        );
        s.push_str(
            "# The number is a DEFAULT. An environment value or a Kconfig / board `.conf`\n\
             # value states a number and WINS; this only fills in what nobody stated.\n#\n",
        );
        s.push_str(
            "# It carries NO headroom, deliberately: it is exactly the declared slot\n\
             # demand, so a stale declaration makes the image fail entity creation with\n\
             # `ExecutorFull` naming this knob, rather than being absorbed silently.\n",
        );
        s.push_str(&format!(
            "set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION {ENTITY_INVENTORY_SCHEMA_VERSION})\n"
        ));
        s.push_str(&format!(
            "set(NROS_ENTITY_INVENTORY_SOURCE \"{}\")\n",
            cmake_escape(&self.source)
        ));
        s.push_str(&format!(
            "set(NROS_ENTITY_INVENTORY_STATUS \"{}\")\n",
            derivation.tag()
        ));
        s.push_str(&format!(
            "set(NROS_ENTITY_INVENTORY_COMPONENT_COUNT {})\n",
            self.components.len()
        ));
        match &derivation {
            Derivation::Refused { reason } => {
                s.push_str(&format!(
                    "set(NROS_ENTITY_INVENTORY_REASON \"{}\")\n",
                    cmake_escape(reason)
                ));
                s.push_str("# No knob is derived. Every one keeps its configured value.\n");
            }
            Derivation::Derived(k) => {
                s.push_str(&format!(
                    "set(NROS_ENTITY_INVENTORY_ENTITY_TOTAL {})\n",
                    k.entity_total
                ));
                for (name, n) in &k.per_kind {
                    let key = name.to_ascii_uppercase();
                    s.push_str(&format!("set(NROS_ENTITY_COUNT_{key} {n})\n"));
                }
                s.push_str("# Where the slots came from -- pkg::component = entities/slots.\n");
                for (pkg, comp, count, slots) in &k.per_component {
                    s.push_str(&format!(
                        "#   {pkg}::{comp} = {count} entities, {slots} slots\n"
                    ));
                }
                s.push_str(
                    "# A publisher claims NO callback slot (it writes an RmwPublisher into\n\
                     # caller storage and never reaches Executor::next_entry_slot), so the\n\
                     # entity total above is larger than the slot demand below.\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_EXECUTOR_MAX_CBS {})\n",
                    k.max_cbs
                ));
            }
        }
        s
    }

    /// The environment projection -- the carrier that reaches a cargo build.
    ///
    /// One `KEY=VALUE` per line, and NOTHING when the derivation refused: an
    /// absent variable leaves `nros-node/build.rs` on its own default, which is
    /// rung 4 of the precedence ladder and the correct outcome for "no answer".
    pub fn to_env(&self) -> String {
        match self.derive() {
            Derivation::Derived(k) => format!("NROS_EXECUTOR_MAX_CBS={}\n", k.max_cbs),
            Derivation::Refused { .. } => String::new(),
        }
    }
}

/// CMake `set(... "...")` is quote- and backslash-sensitive, and a refusal
/// reason is multi-line prose.
fn cmake_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stated(pkg: &str, comp: &str, specs: &[&str]) -> ComponentEntities {
        let mut decls = Vec::new();
        for s in specs {
            decls.extend(EntityDecl::parse(s).expect("spec parses"));
        }
        ComponentEntities {
            pkg: pkg.to_string(),
            component: comp.to_string(),
            class: format!("{pkg}::{comp}"),
            declaration: Declaration::Stated(decls),
        }
    }

    /// The whole point: a publisher is declared, counted, and claims no slot.
    /// The two numbers differ and both are reported.
    #[test]
    fn a_publisher_is_inventoried_and_claims_no_callback_slot() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("p", "n", &["pub*3", "sub", "timer"]));
        let d = inv.derive();
        let k = d.knobs().expect("derived");
        assert_eq!(k.entity_total, 5);
        assert_eq!(k.max_cbs, 2, "3 publishers claim no slot; sub + timer do");
        assert_eq!(k.per_kind["publisher"], 3);
    }

    /// The refusal that makes an under-report impossible. One un-annotated
    /// component and the WHOLE image derives nothing -- not a total three
    /// components' worth short.
    #[test]
    fn one_undeclared_component_refuses_the_whole_image() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub", "timer"]));
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        match inv.derive() {
            Derivation::Refused { reason } => {
                assert!(reason.contains("b::two"), "names the component: {reason}");
                assert!(
                    reason.contains("ENTITIES NONE"),
                    "names the remedy: {reason}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // And no transport may carry a number.
        assert!(!inv.to_cmake().contains("NROS_DERIVED_EXECUTOR_MAX_CBS"));
        assert_eq!(inv.to_env(), "");
    }

    /// "Creates nothing" and "did not say" are different claims, and only the
    /// first one lets the image derive.
    #[test]
    fn an_explicit_none_is_an_answer_and_absence_is_not() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub"]));
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::None,
        });
        let d = inv.derive();
        assert_eq!(d.knobs().expect("derived").max_cbs, 1);
    }

    /// An image with no components is a refusal, not a zero. A zero would be a
    /// perfectly plausible `MAX_CBS` and it would fail the first registration.
    #[test]
    fn an_empty_image_refuses_rather_than_deriving_zero() {
        let inv = EntityInventory::new("test");
        assert!(matches!(inv.derive(), Derivation::Refused { .. }));
        assert!(!inv.to_json().contains("\"max_cbs\""));
    }

    /// An unknown kind is an ERROR at parse time, never a skipped row: a
    /// skipped row is exactly an under-report wearing a typo.
    #[test]
    fn an_unknown_kind_is_rejected_rather_than_skipped() {
        let err = EntityDecl::parse("subscribtion:std_msgs/msg/Int32").unwrap_err();
        assert!(err.contains("unknown entity kind"), "{err}");
        assert!(err.contains("subscription"), "names the legal set: {err}");
    }

    #[test]
    fn a_declaration_carries_its_type_and_name() {
        let d = EntityDecl::parse("sub:nav_msgs/msg/Odometry:/localization/kinematic_state")
            .unwrap()
            .remove(0);
        assert_eq!(d.kind, EntityKind::Subscription);
        assert_eq!(d.type_name.as_deref(), Some("nav_msgs/msg/Odometry"));
        assert_eq!(d.name.as_deref(), Some("/localization/kinematic_state"));
    }

    #[test]
    fn a_repeat_count_of_zero_is_rejected() {
        assert!(EntityDecl::parse("timer*0").is_err());
    }

    /// The CMake projection is `include()`able and composes: it sets the knob
    /// only when derived, and records the provenance either way.
    #[test]
    fn the_cmake_projection_sets_the_knob_only_when_derived() {
        let mut inv = EntityInventory::new("build/nros-metadata.json");
        inv.insert(stated("a", "one", &["sub*2", "pub*4", "timer"]));
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_ENTITY_INVENTORY_STATUS \"derived\")"));
        assert!(c.contains("set(NROS_DERIVED_EXECUTOR_MAX_CBS 3)"));
        assert!(c.contains("set(NROS_ENTITY_INVENTORY_ENTITY_TOTAL 7)"));
        assert!(c.contains("set(NROS_ENTITY_COUNT_PUBLISHER 4)"));
        assert!(c.contains("a::one = 7 entities, 3 slots"));
        assert!(
            c.contains("set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 1)"),
            "a reader must be able to refuse an unrecognised schema"
        );
    }

    /// A refusal reason is multi-line prose from `derive()`; a raw newline
    /// inside `set(... "...")` is legal CMake but unreadable, and a stray quote
    /// would end the string early.
    #[test]
    fn a_refusal_reason_is_escaped_for_cmake() {
        let mut inv = EntityInventory::new("test");
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::\"Two\"".into(),
            declaration: Declaration::Absent,
        });
        let c = inv.to_cmake();
        let reason_line = c
            .lines()
            .find(|l| l.starts_with("set(NROS_ENTITY_INVENTORY_REASON"))
            .expect("a reason is published");
        assert!(!reason_line.contains("\\n\\n"), "no double escaping");
        assert!(
            reason_line.ends_with("\")"),
            "the string closes: {reason_line}"
        );
        assert!(reason_line.contains("\\\""), "the class quote is escaped");
    }

    /// The env transport is the cargo carrier and it is EMPTY on a refusal:
    /// an absent variable leaves `nros-node/build.rs` on its own default, which
    /// is rung 4 of the ladder.
    #[test]
    fn the_env_transport_is_empty_on_a_refusal() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub", "timer", "service_server"]));
        assert_eq!(inv.to_env(), "NROS_EXECUTOR_MAX_CBS=3\n");
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        assert_eq!(inv.to_env(), "");
    }

    /// Registering the same component twice cannot double-count it: cmake
    /// re-runs `nano_ros_node_register` on every configure, and a workspace
    /// that reaches one package through two `add_subdirectory()` paths is a
    /// shape this tree already has.
    #[test]
    fn a_component_recorded_twice_is_recorded_once() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub*3"]));
        inv.insert(stated("a", "one", &["sub*3"]));
        assert_eq!(inv.len(), 1);
        assert_eq!(inv.derive().knobs().unwrap().max_cbs, 3);
    }

    /// Emission order is stable, so a write-if-changed consumer does not
    /// re-arm a reconfigure on every run.
    #[test]
    fn emission_order_is_stable() {
        let mut a = EntityInventory::new("test");
        a.insert(stated("z", "one", &["sub"]));
        a.insert(stated("a", "two", &["timer"]));
        let mut b = EntityInventory::new("test");
        b.insert(stated("a", "two", &["timer"]));
        b.insert(stated("z", "one", &["sub"]));
        assert_eq!(a.to_cmake(), b.to_cmake());
        assert_eq!(a.to_json(), b.to_json());
    }

    /// The measured island. Its four components, exactly as their ctors read
    /// today, and the two numbers the bring-up conflated.
    ///
    /// 33 entities is what a human counts and what
    /// `docs/roadmap/phase-3-canhubk344-real-silicon.md` recorded; 19 is the
    /// callback-slot demand, because the 14 publishers claim no slot. The board
    /// `.conf` pins 36.
    #[test]
    fn the_island_derives_nineteen_slots_from_thirty_three_entities() {
        let mut inv = EntityInventory::new("island");
        inv.insert(stated(
            "autoware_mrm_handler",
            "mrm_handler",
            &["sub*7", "pub*5", "service_client*2", "timer"],
        ));
        inv.insert(stated(
            "autoware_stop_mode_operator",
            "stop_mode_operator",
            &["pub*4", "sub*3", "timer"],
        ));
        inv.insert(stated(
            "autoware_mrm_comfortable_stop_operator",
            "mrm_comfortable_stop_operator",
            &["service_server", "pub*3", "timer"],
        ));
        inv.insert(stated(
            "autoware_mrm_emergency_stop_operator",
            "mrm_emergency_stop_operator",
            &["sub", "service_server", "pub*2", "timer"],
        ));
        let k = inv.derive().knobs().expect("derived").clone();
        assert_eq!(k.entity_total, 33);
        assert_eq!(k.per_kind["publisher"], 14);
        assert_eq!(k.per_kind["subscription"], 11);
        assert_eq!(k.per_kind["timer"], 4);
        assert_eq!(k.per_kind["service_server"], 2);
        assert_eq!(k.per_kind["service_client"], 2);
        assert_eq!(k.max_cbs, 19);
    }
}
