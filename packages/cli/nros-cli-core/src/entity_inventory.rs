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
//! # The JOIN KEY (phase-403 step 1)
//!
//! Counting entities answers `NROS_EXECUTOR_MAX_CBS` and nothing else. The two
//! SIZE consumers need which types are received, which is why every transport
//! also carries [`EntityInventory::subscribed_types`] and
//! [`EntityInventory::received_types`].
//!
//! They are two different sets on purpose. `subscribed_types` is what
//! `nros_derive_message_bound_knobs` narrows the zenoh payload classes with,
//! because those pools have exactly one allocation site and it is reached only
//! from `declare_subscriber`. `received_types` is wider -- a service server, a
//! service client and both action roles all carry receive buffers -- and it is
//! what the executor arena needs. Collapsing them would either price a
//! service's request against a pool it never allocates from, or leave the arena
//! blind to four kinds. See [`EntityKind::receives`] for how each was read off
//! the arena entry types rather than off the names.
//!
//! The spellings join because both inventories key on `pkg/msg/Name`. That is
//! true for messages and cannot be true for services and actions:
//! `BoundInventory::record_message` is called for `.msg` files and for nothing
//! else, so `pkg/srv/Name_Request` and `pkg/action/Name_Result` have no bound
//! entry to join against. A consumer that meets one must REFUSE, and the CMake
//! reader does.
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
///
/// **2** (phase-403 step 1): the fragment now also carries WHICH TYPES THE
/// IMAGE RECEIVES -- `NROS_ENTITY_SUBSCRIBED_TYPES` and its wider sibling
/// `NROS_ENTITY_RECEIVED_TYPES`, each with its own status. A version-1 fragment
/// carries neither, and a reader that treated its absence as "no type is
/// received" would derive a payload class over an EMPTY set and publish a
/// number smaller than any real sample. That is an incompatible addition even
/// though nothing moved, so it bumps.
pub const ENTITY_INVENTORY_SCHEMA_VERSION: u32 = 2;

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

    /// Does an entity of this kind RECEIVE a serialized payload?
    ///
    /// Read off the arena entry types in
    /// `packages/core/nros-node/src/executor/arena.rs`, which is where a
    /// receive buffer is actually spelled -- not off the names, which mislead
    /// in both directions (a service CLIENT receives; an action CLIENT receives
    /// three different things).
    ///
    /// * `Subscription` -- the topic sample. `SubBufferedRawCEntry` and its
    ///   siblings.
    /// * `ServiceServer` -- the REQUEST. `SrvRawEntry<REQ_BUF, REPLY_BUF>`
    ///   carries a `req_buffer`.
    /// * `ServiceClient` -- the REPLY. `ServiceClientRawArenaEntry<REPLY_BUF>`
    ///   carries a `reply_buffer`.
    /// * `ActionServer` -- three: the SendGoal request, the GetResult request
    ///   and the CancelGoal request.
    ///   `ActionServerRawArenaEntry<GOAL_BUF, RESULT_BUF, FEEDBACK_BUF, _>`.
    /// * `ActionClient` -- three: the goal RESPONSE, the result RESPONSE and
    ///   the feedback message. `ActionClientRawArenaEntry` has the same three
    ///   const buffers, which is the clearest statement that "client" says
    ///   nothing about direction.
    /// * `Publisher` -- no. It SERIALIZES into a per-call stack array
    ///   (`DEFAULT_TX_BUF` in `executor/types.rs`), which is a transmit buffer
    ///   and a different question.
    /// * `Timer`, `GuardCondition` -- no payload at all.
    ///
    /// This is the SEMANTIC predicate. It is deliberately wider than
    /// [`Self::receives_topic_sample`], because the two answer different
    /// questions and collapsing them is how a buffer gets sized too small.
    pub fn receives(self) -> bool {
        match self {
            EntityKind::Subscription
            | EntityKind::ServiceServer
            | EntityKind::ServiceClient
            | EntityKind::ActionServer
            | EntityKind::ActionClient => true,
            EntityKind::Publisher | EntityKind::Timer | EntityKind::GuardCondition => false,
        }
    }

    /// Does an entity of this kind draw from the backend's TOPIC PAYLOAD
    /// pools -- the two statically sized classes
    /// `NROS_SUBSCRIBER_BUFFER_SIZE` / `NROS_SUBSCRIBER_LARGE_SIZE` size?
    ///
    /// Only a subscription, and that is MEASURED rather than assumed: in
    /// `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs` the pools
    /// `SMALL_PAYLOADS` / `LARGE_PAYLOADS` are reached through exactly one
    /// allocation, `alloc_payload_block(rx_buffer_hint)`, and it has exactly
    /// one caller -- the `declare_subscriber` path. A service server's request
    /// buffer and an action client's feedback buffer are real receive buffers
    /// and neither is one of these blocks; they are sized by other knobs.
    ///
    /// So narrowing the payload classes to subscriptions is not an
    /// under-count. Including the other receiving kinds would not make the
    /// number safer -- it would make it describe a pool those entities never
    /// allocate from.
    pub fn receives_topic_sample(self) -> bool {
        matches!(self, EntityKind::Subscription)
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

/// Which types an image RECEIVES, and how many entities receive each.
///
/// The count is per ENTITY and not per type, because the consumer that needs
/// it counts blocks: `NROS_MAX_LARGE_SUBSCRIBERS` is how many large payload
/// BLOCKS the backend reserves, and two subscriptions on one large type need
/// two. Deduplicating to a type set would under-reserve by exactly the
/// duplicates.
///
/// `Refused` carries prose and NO list, for the reason [`Derivation`] does: a
/// consumer either reads a set this module resolved or reads nothing. An empty
/// list is a legitimate ANSWER ("this image receives nothing of that shape")
/// and must never be confused with "nobody said".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceivedTypes {
    /// `(type_name, receiving entity count)`, sorted by `type_name` so the
    /// artifact is byte-stable and a write-if-changed keeps mtimes still.
    Resolved(Vec<(String, usize)>),
    Refused {
        reason: String,
    },
}

impl ReceivedTypes {
    pub fn tag(&self) -> &'static str {
        match self {
            ReceivedTypes::Resolved(_) => "resolved",
            ReceivedTypes::Refused { .. } => "refused",
        }
    }

    pub fn types(&self) -> Option<&[(String, usize)]> {
        match self {
            ReceivedTypes::Resolved(v) => Some(v),
            ReceivedTypes::Refused { .. } => None,
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

    /// The types this image receives THROUGH THE TOPIC PAYLOAD POOLS -- the
    /// join key for the zenoh payload classes (phase-403 step 1).
    ///
    /// Filters the declaration to [`EntityKind::receives_topic_sample`], which
    /// is subscriptions, and counts one per ENTITY.
    pub fn subscribed_types(&self) -> ReceivedTypes {
        self.types_received_by(EntityKind::receives_topic_sample, "subscribed")
    }

    /// Every type this image receives, over every receiving kind
    /// ([`EntityKind::receives`]).
    ///
    /// WIDER than [`Self::subscribed_types`] and published beside it because
    /// the two consumers differ: the payload classes size a pool only
    /// subscriptions allocate from, while the executor ARENA charges a receive
    /// buffer for a service server, a service client and both action roles as
    /// well. Emitting only the narrow set would leave the arena's derivation
    /// (step 3) to re-derive it, and a second derivation is how two green
    /// tools come to disagree.
    pub fn received_types(&self) -> ReceivedTypes {
        self.types_received_by(EntityKind::receives, "received")
    }

    /// The one implementation behind the two views above.
    ///
    /// REFUSES in two cases, and both are "the answer would be short":
    ///
    /// 1. The image's own composition refused -- some component declared no
    ///    `ENTITIES` at all. Its subscriptions are then unknown, and a set
    ///    composed over the components that DID answer is a subset of what the
    ///    image receives.
    /// 2. A matching entity carries no `type_name`. A count needs no type and
    ///    `MAX_CBS` derives happily without one, but a SIZE does: an untyped
    ///    receiving entity is a payload of unknown size, and pricing the rest
    ///    would publish a maximum a real sample can exceed.
    fn types_received_by(&self, matches: fn(EntityKind) -> bool, what: &str) -> ReceivedTypes {
        if let Derivation::Refused { reason } = self.derive() {
            return ReceivedTypes::Refused {
                reason: format!(
                    "the entity inventory itself did not compose, so the {what} type set would \
                     be a subset of what this image receives:\n{reason}"
                ),
            };
        }

        let mut untyped: Vec<String> = Vec::new();
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for c in self.components() {
            for e in c.declaration.entities() {
                if !matches(e.kind) {
                    continue;
                }
                match &e.type_name {
                    Some(t) => *counts.entry(t.clone()).or_insert(0) += 1,
                    None => untyped.push(format!(
                        "    {}::{} declares a `{}`{} with no type",
                        c.pkg,
                        c.component,
                        e.kind.tag(),
                        match &e.name {
                            Some(n) => format!(" on `{n}`"),
                            None => String::new(),
                        }
                    )),
                }
            }
        }

        if !untyped.is_empty() {
            untyped.dedup();
            return ReceivedTypes::Refused {
                reason: format!(
                    "{} receiving entities state no type, so the size of what they receive is \
                     unknown:\n{}\nA count does not need the type and NROS_EXECUTOR_MAX_CBS still \
                     derives; a payload SIZE does. State it as `<kind>:<pkg>/msg/<Name>[:<name>]` \
                     in the component's `nano_ros_node_register(... ENTITIES ...)`.",
                    untyped.len(),
                    untyped.join("\n")
                ),
            };
        }

        ReceivedTypes::Resolved(counts.into_iter().collect())
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
        // The join key, on the canonical transport too. Same three states as
        // the CMake projection: a `status` is always present and the list is
        // present only when it resolved.
        for (key, r) in [
            ("subscribed_types", self.subscribed_types()),
            ("received_types", self.received_types()),
        ] {
            let mut m = serde_json::Map::new();
            m.insert("status".into(), r.tag().into());
            match &r {
                ReceivedTypes::Refused { reason } => {
                    m.insert("reason".into(), reason.clone().into());
                }
                ReceivedTypes::Resolved(v) => {
                    m.insert(
                        "types".into(),
                        serde_json::Value::Object(
                            v.iter().map(|(t, n)| (t.clone(), (*n).into())).collect(),
                        ),
                    );
                    m.insert(
                        "entity_count".into(),
                        v.iter().map(|(_, n)| *n).sum::<usize>().into(),
                    );
                }
            }
            doc.insert(key.into(), serde_json::Value::Object(m));
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

        // phase-403 step 1 -- the JOIN KEY. `nros_derive_message_bound_knobs`
        // prices a type; these two say which types this image RECEIVES, which
        // is the half it cannot know. Emitted in BOTH branches: a refusal here
        // is a fact a reader must act on, and an absent variable would read as
        // "no type is received" -- a payload class derived over an empty set.
        s.push_str(&render_received(
            "SUBSCRIBED",
            "the types SUBSCRIPTIONS receive. These and only these allocate from the\n\
             # backend's two topic payload classes (one `alloc_payload_block` call site,\n\
             # reached only from `declare_subscriber`), so they are the join key for\n\
             # NROS_SUBSCRIBER_BUFFER_SIZE / _LARGE_SIZE / NROS_MAX_LARGE_SUBSCRIBERS.\n\
             # The count is per ENTITY, not per type: two subscriptions on one large type\n\
             # need two blocks.",
            &self.subscribed_types(),
        ));
        s.push_str(&render_received(
            "RECEIVED",
            "every type this image receives, over every receiving kind -- a service\n\
             # SERVER receives requests, a service CLIENT receives replies, and an action\n\
             # server and action client each receive three things. WIDER than the\n\
             # subscribed set above; it is what the executor arena needs, not the payload\n\
             # classes.",
            &self.received_types(),
        ));
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

/// One received-type view, as CMake.
///
/// `NROS_ENTITY_<WHAT>_TYPES_STATUS` is always set, so a reader can tell
/// "resolved to nothing" from "refused" from "this fragment predates the
/// field" -- three states that license three different actions and that an
/// absent list collapses into one.
fn render_received(what: &str, prose: &str, r: &ReceivedTypes) -> String {
    let mut s = format!("# {prose}\n");
    s.push_str(&format!(
        "set(NROS_ENTITY_{what}_TYPES_STATUS \"{}\")\n",
        r.tag()
    ));
    match r {
        ReceivedTypes::Refused { reason } => {
            s.push_str(&format!(
                "set(NROS_ENTITY_{what}_TYPES_REASON \"{}\")\n",
                cmake_escape(reason)
            ));
            s.push_str(&format!(
                "# No {} type set. A consumer that needs one must REFUSE, never fall back\n\
                 # to a wider set -- a wider set is a different question with a different\n\
                 # answer.\n",
                what.to_ascii_lowercase()
            ));
        }
        ReceivedTypes::Resolved(v) => {
            let names: Vec<&str> = v.iter().map(|(t, _)| t.as_str()).collect();
            s.push_str(&format!(
                "set(NROS_ENTITY_{what}_TYPES \"{}\")\n",
                names.join(";")
            ));
            let pairs: Vec<String> = v.iter().map(|(t, n)| format!("{t}={n}")).collect();
            s.push_str(&format!(
                "set(NROS_ENTITY_{what}_TYPE_COUNTS \"{}\")\n",
                pairs.join(";")
            ));
            let total: usize = v.iter().map(|(_, n)| *n).sum();
            s.push_str(&format!("set(NROS_ENTITY_{what}_ENTITY_COUNT {total})\n"));
        }
    }
    s
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
            c.contains(&format!(
                "set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION {ENTITY_INVENTORY_SCHEMA_VERSION})"
            )),
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

    // -----------------------------------------------------------------
    // phase-403 step 1 -- the JOIN KEY.
    // -----------------------------------------------------------------

    /// Which kinds RECEIVE is the decision this step turns on, and the two
    /// predicates are deliberately different sets. Pinning both here is what
    /// stops someone "simplifying" one into the other: widening
    /// `receives_topic_sample` would price a service's request against a pool
    /// it never allocates from, and narrowing `receives` would leave the arena
    /// blind to four kinds that carry receive buffers.
    #[test]
    fn a_client_receives_and_a_publisher_does_not() {
        for k in [
            EntityKind::Subscription,
            EntityKind::ServiceServer,
            EntityKind::ServiceClient,
            EntityKind::ActionServer,
            EntityKind::ActionClient,
        ] {
            assert!(k.receives(), "{} receives a payload", k.tag());
        }
        for k in [
            EntityKind::Publisher,
            EntityKind::Timer,
            EntityKind::GuardCondition,
        ] {
            assert!(!k.receives(), "{} receives nothing", k.tag());
        }
        // Only a subscription draws from the topic payload pools -- one
        // `alloc_payload_block` call site, reached only from
        // `declare_subscriber`.
        for k in ALL_ENTITY_KINDS {
            assert_eq!(
                k.receives_topic_sample(),
                *k == EntityKind::Subscription,
                "{} and the payload pools",
                k.tag()
            );
        }
    }

    /// The join key counts ENTITIES, not distinct types. Two subscriptions on
    /// one large type need two large payload BLOCKS, and a deduplicated type
    /// set would reserve one.
    #[test]
    fn the_subscribed_set_counts_entities_per_type() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/a",
                "sub:std_msgs/msg/Int32:/b",
                "sub:nav_msgs/msg/Odometry:/c",
                "pub:sensor_msgs/msg/Image:/d",
                "timer",
            ],
        ));
        let types = inv.subscribed_types();
        assert_eq!(
            types.types().expect("resolved"),
            &[
                ("nav_msgs/msg/Odometry".to_string(), 1),
                ("std_msgs/msg/Int32".to_string(), 2),
            ],
            "two subscriptions on Int32, one on Odometry, and the PUBLISHED \
             Image is not in the set"
        );
    }

    /// A service SERVER receives requests and a service CLIENT receives
    /// replies, so both are in the wider set -- and neither is in the payload
    /// pools' set. The two views are what keep the arena (step 3) from
    /// re-deriving this and disagreeing.
    #[test]
    fn the_received_set_is_wider_than_the_subscribed_one() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/t",
                "service_server:demo/srv/Op_Request:/s",
                "service_client:demo/srv/Op_Response:/c",
                "pub:std_msgs/msg/Bool:/p",
            ],
        ));
        let sub: Vec<String> = inv
            .subscribed_types()
            .types()
            .expect("resolved")
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
        assert_eq!(sub, vec!["std_msgs/msg/Int32".to_string()]);
        let recv: Vec<String> = inv
            .received_types()
            .types()
            .expect("resolved")
            .iter()
            .map(|(t, _)| t.clone())
            .collect();
        assert_eq!(
            recv,
            vec![
                "demo/srv/Op_Request".to_string(),
                "demo/srv/Op_Response".to_string(),
                "std_msgs/msg/Int32".to_string(),
            ],
            "a server's request and a client's reply are both received"
        );
        // And the publisher is in neither.
        assert!(!recv.contains(&"std_msgs/msg/Bool".to_string()));
    }

    /// An untyped SUBSCRIPTION refuses the set rather than being skipped. A
    /// skipped row is an under-report, and here it would size a payload class
    /// from the types that happened to be annotated.
    #[test]
    fn an_untyped_subscription_refuses_the_set_but_not_the_slot_count() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub:std_msgs/msg/Int32:/t", "sub"]));
        match inv.subscribed_types() {
            ReceivedTypes::Refused { reason } => {
                assert!(reason.contains("a::one"), "names the component: {reason}");
                assert!(reason.contains("subscription"), "names the kind: {reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // MAX_CBS does not need the type, so it still derives -- the two
        // questions have different inputs and different answers.
        assert_eq!(inv.derive().knobs().expect("derived").max_cbs, 2);
        // A timer has no type and that is not a refusal.
        let mut ok = EntityInventory::new("test");
        ok.insert(stated(
            "a",
            "one",
            &["sub:std_msgs/msg/Int32:/t", "timer*3"],
        ));
        assert!(ok.subscribed_types().types().is_some());
    }

    /// An image whose composition refused has NO subscribed set either. The
    /// un-annotated component's subscriptions are unknown, so a set composed
    /// over the rest is a subset of what the image receives -- and a payload
    /// class derived from a subset is too small.
    #[test]
    fn an_incomplete_image_has_no_subscribed_set() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub:std_msgs/msg/Int32:/t"]));
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        assert!(matches!(
            inv.subscribed_types(),
            ReceivedTypes::Refused { .. }
        ));
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS \"refused\")"));
        assert!(
            !c.contains("set(NROS_ENTITY_SUBSCRIBED_TYPES "),
            "a refusal must publish no set at all: {c}"
        );
    }

    /// An image that declares entities and subscribes to NOTHING resolves to
    /// an EMPTY set, which is an answer and not a refusal: its payload pools
    /// are genuinely unused. The status is what tells the two apart, so the
    /// fragment must always carry one.
    #[test]
    fn a_subscriber_less_image_resolves_to_an_empty_set() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["pub:std_msgs/msg/Bool:/p", "timer"]));
        assert_eq!(inv.subscribed_types().types().expect("resolved").len(), 0);
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS \"resolved\")"));
        assert!(c.contains("set(NROS_ENTITY_SUBSCRIBED_TYPES \"\")"));
        assert!(c.contains("set(NROS_ENTITY_SUBSCRIBED_ENTITY_COUNT 0)"));
    }

    /// The CMake projection carries the join key in the shape
    /// `_nros_bounds_join_subscribed` parses: a `;` list of names and a
    /// parallel `;` list of `type=count`.
    #[test]
    fn the_cmake_projection_carries_the_join_key() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:nav_msgs/msg/Odometry:/k",
                "sub:std_msgs/msg/Int32:/t",
                "sub:std_msgs/msg/Int32:/u",
            ],
        ));
        let c = inv.to_cmake();
        assert!(c.contains(
            "set(NROS_ENTITY_SUBSCRIBED_TYPES \"nav_msgs/msg/Odometry;std_msgs/msg/Int32\")"
        ));
        assert!(c.contains(
            "set(NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS \"nav_msgs/msg/Odometry=1;std_msgs/msg/Int32=2\")"
        ));
        assert!(c.contains("set(NROS_ENTITY_SUBSCRIBED_ENTITY_COUNT 3)"));
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
