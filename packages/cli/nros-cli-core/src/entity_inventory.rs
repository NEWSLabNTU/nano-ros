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
//! `NROS_CREATE_WALL_TIMER`) do know the kind and the type `M` -- but anything they
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

// phase-454 W3 -- the QoS VALUE vocabulary, taken from the module that already
// owned it for `qos_overrides.*`. Parsing `best_effort` a second time here is
// how a second vocabulary starts; see that module's header.
use nros_orchestration_ir::qos_override::{
    QoSDurabilityPolicy, QoSHistoryPolicy, QoSReliabilityPolicy, durability_spelling,
    history_spelling, parse_durability, parse_history, parse_reliability, reliability_spelling,
};

// phase-454 W8 -- `buffer:` and the derivation it selects. NOT in the module
// above: `buffer` never reaches a `qos_overrides.*` parameter, no runtime folds
// it into a `QoSProfile`, and it is not a QoS policy at all (RFC-0100 D9).
use crate::queue_depth::{
    BufferDiagnostic, BufferDiscipline, NoDefault, RateMilliHz, depth_default, diagnose,
};

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
///
/// **3** (phase-403 step 2): the declared QoS DEPTHS --
/// `NROS_ENTITY_DECLARED_DEPTHS` and, load-bearing beside it,
/// `NROS_ENTITY_UNDECLARED_DEPTH_COUNT`. Bumps on the same argument: a
/// version-2 fragment carries no depth at all, and a reader that took the
/// absent list for "every endpoint is depth 0" would size an arena an order of
/// magnitude short. Absence has to be distinguishable from zero here too.
///
/// **4** (phase-454 W2): the depth table SPLIT BY KIND. A publisher's declared
/// depth now travels, in `NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER` beside its own
/// `_COUNT` and `NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER`, and
/// `NROS_ENTITY_DECLARED_DEPTHS` is correspondingly SUBSCRIPTIONS only. Two
/// reasons to bump rather than to add quietly. The second variable is the
/// familiar one: a reader that took its absence from a version-3 fragment for
/// "no publisher in this image declared a depth" would be reading an older
/// CLI's silence as an answer. The first is the narrowing -- a version-3
/// fragment's `NROS_ENTITY_DECLARED_DEPTHS` was defined over every
/// depth-carrying kind, and while no in-tree producer ever put a non-
/// subscription row in one, the variable's DEFINITION changed and a reader
/// cannot tell which definition it is holding without the version.
///
/// **5** (phase-454 W3, issue 1256): the OTHER THREE QoS policies. The contract
/// has been able to state `reliability`, `durability` and `history` per endpoint
/// for four phases; `depth_of` read the depth and dropped them. They now travel
/// as `NROS_ENTITY_DECLARED_{RELIABILITY,DURABILITY,HISTORY}[_PUBLISHER]` with
/// per-kind undeclared counts and their own `_QOS_POLICY_STATUS`. Bumps for the
/// familiar reason -- absence of a list in a version-4 fragment is an older
/// CLI's silence, not "no endpoint declared" -- and for a second one that is
/// NOT merely additive: `NROS_ENTITY_DECLARED_DEPTH_STATUS` can now read
/// `refused` for a reason a version-4 reader has never seen, `history =
/// keep_all` on some endpoint. A KEEP_ALL queue has no static bound, so a depth
/// stated beside it prices nothing (RFC-0100 D6), and a reader that kept using
/// the old list would size from a number that has stopped meaning what it said.
///
/// **6** (phase-454 W8, RFC-0100 D9): the depth table's rows now have a
/// PROVENANCE, and `NROS_ENTITY_DERIVED_DEPTHS` / `_COUNT` publish it. Bumps for
/// the familiar reason and for one that is, again, not additive.
///
/// The familiar half: an absent `NROS_ENTITY_DERIVED_DEPTHS` in a version-5
/// fragment is an older CLI's silence, not "this image derived no depth", and a
/// reader that took it for the second would report a derived number as stated.
///
/// The half that is not additive: `NROS_ENTITY_DECLARED_DEPTHS`'s DEFINITION
/// moved again. In version 5 every entry in it was a number a contract author
/// wrote; from version 6 an entry may be one this CLI derived from a
/// `buffer: queue` endpoint's publish and drain rates. Same variable, same
/// shape, different warrant -- and a consumer that asserts against the list
/// (rather than sizing from it) must now read the derived subset and exclude
/// it. The in-tree asserting consumer already does, one function over: see
/// [`EntityInventory::to_declared_qos_header`] and [`DepthSource`].
pub const ENTITY_INVENTORY_SCHEMA_VERSION: u32 = 6;

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

    /// Does an entity of this kind have a QoS HISTORY DEPTH at all?
    ///
    /// phase-403 step 2. `@depth=N` is a QoS attribute, and a timer and a guard
    /// condition are not endpoints -- they carry no QoS, so a depth on one is a
    /// statement about nothing. It is REJECTED rather than ignored, for the
    /// reason an unknown kind is: a silently ignored attribute is a declaration
    /// the author believes they made.
    ///
    /// Every other kind does carry one. A publisher's depth sizes no receive
    /// buffer, so nothing reads it yet, but it is a real QoS field and
    /// forbidding it here would make the grammar say something false.
    pub fn carries_qos_depth(self) -> bool {
        !matches!(self, EntityKind::Timer | EntityKind::GuardCondition)
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
/// `depth` is the QoS HISTORY DEPTH the author declared (phase-403 step 2), and
/// it is `Option` for the reason every other field here is: the arena's
/// per-subscription cost is `(depth + 1) * bound + (depth + 1) * 8`, so a
/// DEFAULT is wrong by up to 10x in either direction -- assuming the ROS
/// default 10 inflates an image that states 1 tenfold, and assuming 1
/// UNDER-sizes one that took the default. `None` means NOBODY SAID, and a
/// consumer that needs a depth must refuse on it. It must never read as 0.
///
/// # The other three policies (phase-454 W3, issue 1256)
///
/// `reliability`, `durability` and `history` ride here for the same reason and
/// under the same rule. Each is `Option` and `None` means NOBODY SAID: an
/// undeclared `reliability` is NOT `best_effort`, and a consumer that needs one
/// -- XRCE's two 64 KiB `*_reliable_buf` are the first -- must refuse rather
/// than assume, on the safe side of its own question.
///
/// They were stated, resolved and dropped for four phases: the contract schema
/// has carried all four since `QosDecl` existed, `effective_qos` resolves them
/// per endpoint into the SystemModel, and `from_model` read `qos.depth` and
/// nothing else. The one with a live defect behind it is `history`: a
/// `keep_all` endpoint was priced at whatever `depth` said, which is a silent
/// UNDER-size (RFC-0100 D6) -- see [`EntityInventory::declared_depths`], which
/// refuses on it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityDecl {
    pub kind: EntityKind,
    pub type_name: Option<String>,
    pub name: Option<String>,
    pub depth: Option<u32>,
    /// phase-454 W3 -- `reliable` / `best_effort`, or `None` for "nobody said".
    pub reliability: Option<QoSReliabilityPolicy>,
    /// phase-454 W3 -- `volatile` / `transient_local`, or `None`.
    pub durability: Option<QoSDurabilityPolicy>,
    /// phase-454 W3 -- `keep_last` / `keep_all`, or `None`.
    pub history: Option<QoSHistoryPolicy>,
    /// phase-454 W8 -- `latest` / `queue`, or `None` for "nobody said".
    ///
    /// NOT a QoS policy and not a size (RFC-0100 D9): it is the FAULT MODE the
    /// author declared for a `state: true` subscription. `None` must never read
    /// as `Latest` even though `latest` is the schema's default when the key is
    /// absent -- see [`crate::queue_depth::BufferDiscipline`].
    pub buffer: Option<BufferDiscipline>,
    /// phase-454 W8 -- how fast this endpoint's topic is published.
    ///
    /// The numerator of the queue-depth derivation. Read from the model's
    /// `contracts.topics.<topic>.rate_hz`, or failing that from the
    /// `min_rate_hz` the topic's publishers promise.
    pub publish_rate: Option<RateMilliHz>,
    /// phase-454 W8 -- how fast the consuming timer drains it.
    ///
    /// The denominator. Authored as `paths.<p>.trigger: { timer: { rate_hz } }`
    /// and NOT carried by the SystemModel (issue 1339); what reaches this
    /// reader today is the `min_rate_hz` of what the node's timer paths
    /// publish, which is the model's own convention for a periodic path's rate
    /// (`nros_orchestration_ir::mapper_input::pub_rate_hz`).
    pub drain_rate: Option<RateMilliHz>,
}

impl EntityDecl {
    /// A row that states no QoS policy at all.
    ///
    /// Every producer but `from_model` builds one of these -- the `ENTITIES`
    /// grammar models only `@depth=`, and the timer/service rows carry no QoS
    /// by construction. Spelled once so that adding a fifth policy is one
    /// signature change rather than a sweep over thirteen struct literals, and
    /// so that a site which MEANT to state one is the site that does not call
    /// this.
    pub fn bare(kind: EntityKind, type_name: Option<String>, name: Option<String>) -> Self {
        Self {
            kind,
            type_name,
            name,
            depth: None,
            reliability: None,
            durability: None,
            history: None,
            buffer: None,
            publish_rate: None,
            drain_rate: None,
        }
    }

    /// Parse the declaration spelling:
    /// `<kind>[:<type>[:<name>]][@<attr>=<value>...]`.
    ///
    /// `sub:nav_msgs/msg/Odometry:/localization/kinematic_state@depth=10`,
    /// `timer`, `publisher:autoware_vehicle_msgs/msg/GearCommand`.
    ///
    /// A `*N` suffix on the kind repeats it: `timer*3`. A repeat count is the
    /// one concession to brevity, and it is on the KIND rather than a separate
    /// argument so a row can never lose its multiplier in transit.
    ///
    /// # Why attributes are NAMED and split off FIRST
    ///
    /// The positional part is parsed `splitn(3, ':')`, so the NAME takes the
    /// rest of the spec and a fourth positional field would be ambiguous
    /// against a topic containing a colon. `@depth=10` is therefore a named
    /// attribute, which also leaves room for `@reliability=`, `@history=` and
    /// `@durability=` without another grammar change.
    ///
    /// The `@` split runs BEFORE the `:` split, so an attribute attaches to the
    /// whole declaration rather than to whichever field happens to be last.
    /// That is safe because neither a ROS type name nor a ROS topic name may
    /// contain `@` (REP-144 allows alphanumerics, `_`, `/`, `~`, `{`, `}`), so
    /// the character cannot occur in the positional part.
    ///
    /// An UNKNOWN attribute is an error, never a skipped one, on this module's
    /// standing rule: a silently ignored declaration is one the author believes
    /// they made.
    pub fn parse(spec: &str) -> Result<Vec<Self>, String> {
        let spec = spec.trim();
        if spec.is_empty() {
            return Err("empty entity declaration".to_string());
        }
        let mut fields = spec.split('@');
        let positional = fields.next().unwrap_or("").trim();
        let mut depth: Option<u32> = None;
        for attr in fields {
            let attr = attr.trim();
            if attr.is_empty() {
                return Err(format!(
                    "entity declaration `{spec}`: an empty `@` attribute states nothing. \
                     Write `@depth=<N>` or drop the `@`."
                ));
            }
            let (key, value) = attr.split_once('=').ok_or_else(|| {
                format!(
                    "entity declaration `{spec}`: attribute `@{attr}` has no value. \
                     Attributes are `@<name>=<value>`, e.g. `@depth=10`."
                )
            })?;
            match key.trim() {
                "depth" => {
                    if depth.is_some() {
                        return Err(format!(
                            "entity declaration `{spec}`: `@depth=` is stated twice. \
                             One entity has one depth."
                        ));
                    }
                    let n: u32 = value.trim().parse().map_err(|_| {
                        format!(
                            "entity declaration `{spec}`: `{}` is not a QoS depth. \
                             It is a positive whole number of samples, e.g. `@depth=10`.",
                            value.trim()
                        )
                    })?;
                    if n == 0 {
                        return Err(format!(
                            "entity declaration `{spec}`: a QoS depth of 0 states nothing -- \
                             KEEP_LAST(0) holds no sample. Omit `@depth=` to say \"not \
                             declared\", which is a different claim and the one that makes a \
                             size consumer REFUSE rather than guess."
                        ));
                    }
                    depth = Some(n);
                }
                other => {
                    return Err(format!(
                        "entity declaration `{spec}`: unknown attribute `@{other}=` -- \
                         expected one of: depth"
                    ));
                }
            }
        }

        let mut parts = positional.splitn(3, ':');
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
        if depth.is_some() && !kind.carries_qos_depth() {
            return Err(format!(
                "entity declaration `{spec}`: a `{}` has no QoS, so `@depth=` says nothing \
                 about it. Drop the attribute.",
                kind.tag()
            ));
        }
        Ok((0..repeat)
            .map(|_| EntityDecl {
                // phase-454 W3 -- the other three policies stay `None` on this
                // road. The grammar models `@depth=` and nothing else, and the
                // contract is the surface that carries the rest (RFC-0100 D3);
                // inventing `@reliability=` here would add a second producer to
                // a grammar phase-454 W9 retires.
                depth,
                ..EntityDecl::bare(
                    kind,
                    type_name.map(str::to_string),
                    name.map(str::to_string),
                )
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
    /// Issue 1270 -- the runtime's own service families, when the bringup
    /// declares them. Default (none) for an inventory built from
    /// `nros-metadata.json` alone, which carries no bringup features.
    infra: InfraServices,
    /// phase-446 W4 -- the contract's `params:`, read from the SystemModel.
    /// Independent of `components`: an image whose entity count refuses can
    /// still have its parameter store sized, and the reverse.
    params: ParamDeclarations,
    /// Issue 1198 -- how many SCHEDULING TIERS the bringup authors
    /// (`execution.tiers`), which is a different declaration from the entity
    /// one and the only artifact that says anything about scheduling contexts.
    /// Zero for an inventory built from a probe, which sees no model.
    ///
    /// See [`DerivedEntityKnobs::max_sc`] for what it is a term in and why the
    /// node count is the other term.
    tiers: usize,
}

/// MIRRORS of the action multipliers in
/// `nros_node::executor::action`. The CLI cannot depend on `nros-node`, so
/// these are copies, and `check-infra-queryable-counts` holds each to the
/// creation calls that decide it -- the same arrangement
/// `ACTION_SERVER_QUERYABLES` already has in `cmd::entity_facts`.
///
/// They exist because a declared action is ONE entity that costs SEVERAL
/// session slots. An author writing `ENTITIES action_server:...` declares one
/// thing; the backend opens three queryables and two publishers for it. A pool
/// sized from the raw per-kind count is short for every image with an action,
/// and short halts the board.
const ACTION_SERVER_QUERYABLES: usize = 3;
const ACTION_SERVER_PUBLISHERS: usize = 2;
const ACTION_CLIENT_SUBSCRIPTIONS: usize = 1;

/// MIRRORS of `nros_node::parameter_services::PARAM_SERVICE_QUERYABLES` and
/// `nros_node::lifecycle_services::LIFECYCLE_SERVICE_QUERYABLES`, held to the
/// creation calls by `check-infra-queryable-counts` exactly as the three above
/// are (issue 1270).
const PARAM_SERVICE_QUERYABLES: usize = 6;
const LIFECYCLE_SERVICE_QUERYABLES: usize = 5;

/// MIRROR of `nros_node::executor::action::ACTION_CLIENT_SERVICE_CLIENTS`,
/// held there by `check-infra-queryable-counts`: the three service clients an
/// action client opens, each of which declares a liveliness token.
const ACTION_CLIENT_SERVICE_CLIENTS: usize = 3;

/// phase-412 -- the liveliness token the zenoh session declares for its OWN
/// node name at open (`ZenohSession::new`, `node_liveliness`), before any
/// component's per-node token. It is dropped only when a per-node token with a
/// DIFFERENT name supersedes it; in the single-node case the component shares
/// the primary name and the two tokens coexist for the life of the session, so
/// this is a permanent slot and not a transient one.
const PRIMARY_NODE_LIVELINESS_TOKENS: usize = 1;

/// Issue 1270 -- the service servers the RUNTIME creates on an image's behalf.
///
/// No component declares them, and for a while that was read as "this
/// inventory cannot see them". It can: the BRINGUP says so. `[param_services]`
/// and `[lifecycle]` (or `features = [...]`) land in the model's
/// `execution.features`, the same fact `nros ws entity-facts` hands the cmake
/// road as `NROS_DECLARED_INFRA_QUERYABLES`, and both read it through
/// [`InfraServices::from_model`] so the two roads cannot disagree about
/// whether a family is in the image.
///
/// Counted into the SESSION pool (`max_queryables`) and never into `MAX_CBS`:
/// both families live outside the executor arena (see the module docs).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfraServices {
    /// The bringup declares `param_services`: six servers on EVERY node
    /// (phase-426 W3), because `ros2 param` addresses nodes.
    pub param_services: bool,
    /// The bringup declares `lifecycle`: five servers, once per executor.
    pub lifecycle: bool,
    /// How many nodes the model resolves. [`Self::param_nodes`] takes the
    /// larger of this and the image's component count, floored at one -- the
    /// executor's own rule (`nodes.len().max(1)` sets).
    pub model_nodes: usize,
}

impl InfraServices {
    /// What a resolved model declares. An unrecognised feature is not one of
    /// these and is ignored, as `entity_facts` has always done.
    pub fn from_model(model: &ros_launch_manifest_model::SystemModel) -> Self {
        Self::from_features(&model.execution.features, model.structure.nodes.len())
    }

    /// Issue 1142 -- the same two names, read from a feature LIST.
    ///
    /// A standalone leaf has no resolved model and states its features in the
    /// SAME key a bringup does (`[system] features`, reaching a model as
    /// `execution.features`). [`Self::from_model`] delegates here so the two
    /// roads cannot come to disagree about which spelling turns a family on.
    pub fn from_features<S: AsRef<str>>(features: &[S], nodes: usize) -> Self {
        let has = |name: &str| features.iter().any(|f| f.as_ref() == name);
        Self {
            param_services: has("param_services"),
            lifecycle: has("lifecycle"),
            model_nodes: nodes,
        }
    }

    /// The `NROS_DECLARED_INFRA_QUERYABLES` spelling.
    ///
    /// The consumer PANICS on a token it does not know
    /// (`nros-zpico-build::infra_queryables`), so these four strings are a
    /// contract -- written in exactly one place, on every road.
    pub fn token(self) -> &'static str {
        match (self.param_services, self.lifecycle) {
            (true, true) => "param+lifecycle",
            (true, false) => "param",
            (false, true) => "lifecycle",
            (false, false) => "none",
        }
    }

    /// Nodes carrying the parameter family on an image of `components`
    /// components; zero when the family is not declared.
    pub fn param_nodes(self, components: usize) -> usize {
        if self.param_services {
            self.model_nodes.max(components).max(1)
        } else {
            0
        }
    }

    /// Queryables (service servers) the two families claim at boot.
    pub fn queryables(self, components: usize) -> usize {
        let lifecycle = if self.lifecycle {
            LIFECYCLE_SERVICE_QUERYABLES
        } else {
            0
        };
        PARAM_SERVICE_QUERYABLES * self.param_nodes(components) + lifecycle
    }

    /// Either source declaring a family declares it: this is a fact about the
    /// bringup, not a count two sources could each get partly right.
    fn union(self, other: Self) -> Self {
        Self {
            param_services: self.param_services || other.param_services,
            lifecycle: self.lifecycle || other.lifecycle,
            model_nodes: self.model_nodes.max(other.model_nodes),
        }
    }
}

/// Issue 1015 -- the smallest a pool that SIZES A FIXED C ARRAY may be.
///
/// Not a property of the derivation: the demand is whatever the image declares,
/// zero included. It is a property of the STORAGE the knob reaches, so it is
/// applied by the consumer that names the knob -- and only by the consumers
/// whose storage is a C array. `ZPICO_MAX_QUERYABLES` at 0 gave a board that
/// transmitted nothing for 15 s with no diagnostic; `XRCE_MAX_SUBSCRIBERS` at 0
/// is 33,296 bytes of heap an image gets back (issue 1033). The same derived
/// number feeds both, so the floor cannot live where the number is made.
pub const C_ARRAY_POOL_FLOOR: usize = 1;

/// Raise a derived demand to what a fixed C array can actually be sized to.
///
/// ONE spelling for the Rust lane, so a second consumer cannot round up by a
/// slightly different rule. The CMake lane's copy is
/// `_nros_c_array_pool_floor()` in `zephyr/cmake/nros_cargo_build.cmake`, and
/// `check-c-array-pool-floors` holds the two to the set of knobs whose arrays
/// carry the `#if ... < 1 / #error` guard.
pub fn c_array_pool_floor(demand: usize) -> usize {
    demand.max(C_ARRAY_POOL_FLOOR)
}

/// The knobs an entity inventory can answer, plus how it got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedEntityKnobs {
    /// `NROS_EXECUTOR_MAX_CBS` -- the total callback-entry slot demand.
    pub max_cbs: usize,
    /// `NROS_EXECUTOR_ACTION_CLIENTS` (issue 0900) -- how many of those slots
    /// the arena must budget at the HEAVY entry size rather than the pub/sub
    /// one. At the defaults that is 18,048 bytes against 3,584, and the arena
    /// is inline on the TASK STACK, so budgeting every slot heavy is 74,240
    /// bytes where a talker needs 16,384.
    ///
    /// Counts action SERVERS as well as clients, though the knob is named for
    /// clients. The knob's real meaning is "slots budgeted at the worst case",
    /// and `build.rs` picked the action client as that worst case when nothing
    /// else was measured. It is not the worst case: the arena demonstrably
    /// stores `ActionServerArenaEntry`, so an action-server image occupies
    /// heavy slots too, and counting only clients would advise it into exactly
    /// the `BufferTooSmall` this derivation exists to avoid. Counting both is
    /// conservative in the safe direction.
    pub heavy_slots: usize,
    /// Every declared entity, slot-claiming or not. NOT the knob: kept because
    /// it is the number a human counts, and because the gap between the two is
    /// the finding.
    pub entity_total: usize,
    /// `NROS_MAX_SUBSCRIBERS` / `NROS_RMW_SUBSCRIBER_SLOTS` -- session
    /// subscriber slots. Declared subscriptions PLUS the feedback subscription
    /// each action client opens.
    ///
    /// Verified against the shim rather than assumed: `ZenohSubscriber::new`
    /// has exactly one caller (`create_subscription`), and the two things that
    /// looked like they might share the pool do not -- the graph cache lives in
    /// its own `graph_cache.sub` field and liveliness tokens in
    /// `liveliness[ZPICO_MAX_LIVELINESS]`. So there is no shim addend.
    pub max_subscribers: usize,
    /// `NROS_MAX_PUBLISHERS` -- declared publishers plus the feedback and
    /// status topics each action server publishes.
    pub max_publishers: usize,
    /// `NROS_MAX_QUERYABLES` -- a service server IS a queryable, and an action
    /// server is [`ACTION_SERVER_QUERYABLES`] of them.
    ///
    /// INCLUDES the runtime's parameter and lifecycle service families when
    /// the bringup declares them ([`InfraServices`], issue 1270):
    /// [`PARAM_SERVICE_QUERYABLES`] per node and
    /// [`LIFECYCLE_SERVICE_QUERYABLES`] per executor. Before that they were
    /// left out as "a feature this inventory cannot see", so every image
    /// declaring `param_services` derived a pool short by six per node, and a
    /// short queryable pool is a registration failure at boot. Still a
    /// DEFAULT rather than a ceiling: a stated knob wins.
    pub max_queryables: usize,
    /// Issue 1270 -- the part of [`Self::max_queryables`] that is the
    /// runtime's own servers rather than the application's. Provenance.
    pub infra_queryables: usize,
    /// Issue 1270 -- how many nodes carry the six parameter services; zero
    /// when the bringup does not declare `param_services`.
    pub param_service_nodes: usize,
    /// `NROS_EXECUTOR_MAX_NODES` -- one node per declared component.
    ///
    /// A `ComponentNode` constructor is one `Node::create` is one node NAME,
    /// and the executor keys node slots by name ("a repeated name must reuse
    /// its record"), so two components sharing a name share a slot and this
    /// OVER-counts by one. Over-counting is the safe direction.
    ///
    /// UNDER-counting has exactly one source: `nros_pubsub_bridge_create`
    /// creates TWO nodes whose names are RUNTIME strings, declared nowhere.
    /// That path now names this knob when the table is full rather than
    /// returning a bare error code, which is what makes deriving it safe -- the
    /// same argument that let `MAX_CBS` derive, where the shortfall surfaces as
    /// `ExecutorFull` naming the knob. An image that bridges states this knob.
    pub max_nodes: usize,
    /// `NROS_MAX_LIVELINESS` / `ZPICO_MAX_LIVELINESS` (phase-412 W2) -- every
    /// liveliness token THIS session declares. Not the peer graph: a remote
    /// node's token lives in the graph cache, and the cache's own liveliness
    /// SUBSCRIBER takes no slot in this array.
    ///
    /// Read off the zenoh shim (`shim/session.rs`), term by term:
    ///
    /// * [`PRIMARY_NODE_LIVELINESS_TOKENS`] -- the session's own node token.
    /// * one per node NAME (`ensure_node_liveliness`, deduplicated by name):
    ///   the larger of [`Self::max_nodes`] and the parameter-service node
    ///   count, plus one when the bringup declares `lifecycle`, whose five
    ///   servers register under the EXECUTOR's node name, which need not be a
    ///   component's.
    /// * one per publisher, subscriber, service server and service client --
    ///   `create_publisher` / `create_subscription` / `create_service` /
    ///   `create_client` each call `declare_entity_liveliness` once. That is
    ///   [`Self::max_publishers`] + [`Self::max_subscribers`] +
    ///   [`Self::max_queryables`] (so the action multipliers and the param /
    ///   lifecycle servers of issue 1270 are already in) + the service
    ///   clients, three of them per action client.
    ///
    /// Timers and guard conditions declare nothing. Every term errs HIGH where
    /// the shim is ambiguous (a component sharing the executor's name reuses
    /// one token; this counts two), and exhaustion is NAMED, not silent: the
    /// C arm printks the knob and the Rust arm logs it (phase-412, PR 749).
    /// Short is not a boot failure -- the entity works and is invisible to
    /// `ros2 node list` -- but it is exactly the silent graph outage issue
    /// 0283 exists to prevent, so the derivation does not gamble on it.
    ///
    /// UNDER-counts only for a bridge (two runtime-named nodes and their
    /// entities, declared nowhere) -- the `max_nodes` exception, one pool over.
    pub max_liveliness: usize,
    /// `NROS_RUNTIME_MAX_CELL_ENTITIES` (issue 1130) -- the per-KIND capacity of
    /// a component cell's registries when its class states no `ENTITY_BOUNDS`.
    ///
    /// The max over components of the max over the five kinds a cell
    /// registers: publishers, service servers, service clients, action
    /// clients, action servers. Subscriptions and timers reach no cell
    /// registry. One number because the knob is one number: every
    /// knob-capped class gets the same capacity per kind, so it must hold the
    /// largest single kind of the largest component.
    ///
    /// ZERO IS LEGAL and is published unfloored: the registries are Rust
    /// arrays, `EntityBounds::exact(0, 1, 0, 0, 0)` is already the in-tree
    /// spelling for "none of this kind", and an image whose components declare
    /// none of the five simply carries empty registries. A short registry is a
    /// `NodeDeclError::CellRegistryFull` at registration, which names this
    /// knob. An explicit `ENTITY_BOUNDS` still wins: it is per CLASS, and the
    /// knob only sizes the classes that state nothing.
    pub max_cell_entities: usize,
    /// `NROS_EXECUTOR_MAX_SC` -- scheduling-context slots (issue 1198).
    ///
    /// NOT an entity count, and that is the finding rather than an obstacle:
    /// an SC is not created by any component, it is created by the image's
    /// SCHEDULE. Measured over the tree, there are exactly two producers, and
    /// they disagree about how many slots a tier costs:
    ///
    /// * the RUST runtime creates **none**. `apply_tier_sched_policy` ends in
    ///   `set_default_sched_context`, which MUTATES slot 0 -- so a tiered Rust
    ///   boot, which opens one executor per tier, spends one reserved slot per
    ///   executor and no table entry at all.
    /// * the C / C++ entry pack creates **one per tier**, through
    ///   `nros_cpp_create_sched_context_from_policy` into `__nros_sc_ids[N]`,
    ///   where `N` is the resolved tier table's length (`SchedView::n`).
    ///
    /// So the demand is `1 + <SCs the schedule creates>`: the `1` is slot 0,
    /// which `create_sched_context` reserves for the default Fifo context and
    /// never hands out (it searches `1..MAX_SC`).
    ///
    /// The second term is bounded by `max(authored tiers, nodes)`, not by the
    /// authored tier count alone, because a bringup that authors NO tiers does
    /// not thereby have one: `derive_tiers_from_contracts` synthesises a
    /// `derived-<node>` tier per schedulable node, so a model with an empty
    /// `[tiers.*]` can still resolve to one tier per node. Taking the larger
    /// of the two covers both shapes and over-counts for the Rust one, which
    /// is the safe direction.
    ///
    /// UNDER-counting has one source, and it is the same shape as
    /// [`Self::max_nodes`]'s bridge: application code calling
    /// `create_sched_context` (or `nros_executor_create_sched_context`) by
    /// hand, which no artifact describes. Every such path NAMES this knob when
    /// the table is full -- `NodeError::NoSchedContextSlot` spells it, and the
    /// three FFI wrappers log it rather than returning a bare `RET_FULL` --
    /// which is what makes deriving it safe. An image that hand-creates
    /// scheduling contexts states this knob.
    pub max_sc: usize,
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

/// One endpoint that stated a QoS history DEPTH (phase-403 step 2).
///
/// `type_name` is the ROS spelling the declaration used (`pkg/msg/Name`);
/// [`dds_type_name`] is what a C++ TU sees as `M::TYPE_NAME`. Both travel,
/// because the two consumers key differently: the arena joins on the ROS
/// spelling that the bound inventory also uses, and the compile-time check
/// joins on whatever the generated message class actually carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredDepth {
    pub kind: EntityKind,
    pub type_name: String,
    pub topic: String,
    pub depth: u32,
    /// Where this number came from -- phase-454 W8.
    pub source: DepthSource,
}

/// Which rung of RFC-0049's ladder a depth row came from -- phase-454 W8.
///
/// # Why a row has to carry this, and what breaks without it
///
/// The depth table has two consumers and they want OPPOSITE things from a
/// derived default:
///
/// * `nros-node/build.rs::subs_arena` SIZES from it. A default is exactly what
///   it wants -- that is the point of deriving one.
/// * [`EntityInventory::to_declared_qos_header`] ASSERTS from it. Every row it
///   emits becomes a `static_assert` that `NROS_SUBSCRIBE`'s own QoS must match,
///   and a row that disagrees fails the BUILD naming the topic and both numbers.
///
/// A derived default reaching the second consumer would turn a default into a
/// REQUIREMENT: an image that never stated a depth would suddenly have to spell
/// this module's arithmetic at every call site or not compile, and raising the
/// margin by one would break every such image. That is the ladder inverted --
/// the derived rung dictating to the code instead of filling in behind it.
///
/// So the header filters to [`DepthSource::Stated`] and the arena does not.
/// One column, two consumers, and the difference is stated rather than implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthSource {
    /// The contract stated `qos: { depth: N }` on this endpoint.
    Stated,
    /// Nobody stated one, and it was derived from the endpoint's publish and
    /// drain rates because it declared `buffer: queue` (RFC-0100 D9).
    DerivedFromRates,
}

impl DepthSource {
    pub fn tag(self) -> &'static str {
        match self {
            DepthSource::Stated => "stated",
            DepthSource::DerivedFromRates => "derived_from_rates",
        }
    }
}

/// One subscription's queue-depth default and what decided it -- phase-454 W8.
///
/// Carries the INPUTS beside the outcome on purpose. A row saying only "no
/// default" sends an author to read the contract and guess which of three facts
/// was missing; a row that also shows which rates arrived answers it. This is
/// the same rule the `keep_all` refusal follows one view over -- name the
/// endpoints, not the count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueDepthDefault {
    pub topic: String,
    pub type_name: Option<String>,
    pub buffer: Option<BufferDiscipline>,
    pub stated_depth: Option<u32>,
    pub publish_rate: Option<RateMilliHz>,
    pub drain_rate: Option<RateMilliHz>,
    /// `Ok(depth)` when a default was derived; `Err` naming which rung stopped
    /// it -- a stated depth above, a discipline that is not `queue`, or a rate
    /// that never arrived.
    pub outcome: Result<u32, NoDefault>,
}

impl QueueDepthDefault {
    /// One line an author can act on. Names the topic, the outcome and, on a
    /// refusal, the reason [`NoDefault`] gives for it.
    pub fn line(&self) -> String {
        match &self.outcome {
            Ok(depth) => format!(
                "{}: depth {depth} derived from {} in / {} drained ({})",
                self.topic,
                self.publish_rate
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "?".into()),
                self.drain_rate
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "?".into()),
                DepthSource::DerivedFromRates.tag(),
            ),
            Err(reason) => format!("{}: no derived depth -- {}", self.topic, reason.reason()),
        }
    }
}

/// Every declared depth in an image, plus how many endpoints did NOT state one.
///
/// The undeclared COUNT is the load-bearing field. `Resolved { rows: [] }` and
/// `Resolved { rows: [...], undeclared: 4 }` are different facts: the first is
/// an image that has not opted in at all, the second one that opted in
/// partially, and a size consumer must refuse on both while a compile-time
/// check must fire on neither. Reporting only the rows would collapse them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredDepths {
    Resolved {
        /// Sorted by `(kind, type_name, topic)` so the artifact is byte-stable.
        /// The kind leads because a publisher and a subscription on one topic
        /// agree on the other two (phase-454 W2), and every consumer of this
        /// table narrows to one kind.
        rows: Vec<DeclaredDepth>,
        /// Endpoints that COULD carry a depth ([`EntityKind::carries_qos_depth`])
        /// and stated none. NOT zero-by-default: this is the number that says
        /// "nobody said" for the rest of the image.
        undeclared: usize,
        /// The same count restricted to SUBSCRIPTIONS -- issue 1227.
        ///
        /// `undeclared` spans every kind that can carry a depth, and a
        /// publisher's depth "sizes no receive buffer, so nothing reads it yet"
        /// ([`EntityKind::carries_qos_depth`] says so itself). A consumer that
        /// sizes only the SUBSCRIPTION term therefore cannot use the broad
        /// count: on the reference island it is 18 -- fourteen publishers and
        /// the service endpoints -- while all eleven subscriptions declare, and
        /// refusing on it keeps that image on the worst case forever for
        /// endpoints the term does not price.
        undeclared_subscriptions: usize,
        /// The same count restricted to PUBLISHERS -- phase-454 W2.
        ///
        /// A publisher's depth now travels (`PubContract::qos.depth`), and the
        /// consumer that will price it -- publisher-side sample retention for
        /// `transient_local` durability -- is a different term from the
        /// subscription arena, over a different set of endpoints. It therefore
        /// needs its own "did EVERY publisher declare?" question, for the exact
        /// reason issue 1227 gave the subscription term one: a count over the
        /// whole image answers neither term, because one unannotated endpoint
        /// of the OTHER kind pins both on the worst case forever.
        undeclared_publishers: usize,
    },
    Refused {
        reason: String,
    },
}

impl DeclaredDepths {
    pub fn tag(&self) -> &'static str {
        match self {
            DeclaredDepths::Resolved { .. } => "resolved",
            DeclaredDepths::Refused { .. } => "refused",
        }
    }

    pub fn rows(&self) -> Option<&[DeclaredDepth]> {
        match self {
            DeclaredDepths::Resolved { rows, .. } => Some(rows),
            DeclaredDepths::Refused { .. } => None,
        }
    }
}

/// The three QoS policies that are NOT a depth -- phase-454 W3, issue 1256.
///
/// One enumerated once, so a consumer that prices only one of them still reads
/// a table with the join key ([`DeclaredDepth`]'s `(kind, type, topic)`) and
/// still has to answer "did every endpoint state it?" for its own policy. The
/// three are separate questions and the counts below are kept separately for
/// exactly that reason: an image where every subscription states `reliability`
/// and none states `durability` can size XRCE's reliable buffers and must
/// refuse a transient-local retention budget, and one count over "any policy"
/// would answer neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QosPolicyKind {
    Reliability,
    Durability,
    History,
}

/// Every policy in [`QosPolicyKind`], for a consumer that walks them.
pub const ALL_QOS_POLICY_KINDS: &[QosPolicyKind] = &[
    QosPolicyKind::Reliability,
    QosPolicyKind::Durability,
    QosPolicyKind::History,
];

impl QosPolicyKind {
    /// The lower-case name, as it is written in a contract and in the JSON.
    pub fn tag(self) -> &'static str {
        match self {
            QosPolicyKind::Reliability => "reliability",
            QosPolicyKind::Durability => "durability",
            QosPolicyKind::History => "history",
        }
    }

    /// The CMake variable INFIX -- `NROS_ENTITY_DECLARED_<INFIX>` and
    /// `NROS_ENTITY_UNDECLARED_<INFIX>_COUNT_<KIND>`.
    pub fn cmake_infix(self) -> &'static str {
        match self {
            QosPolicyKind::Reliability => "RELIABILITY",
            QosPolicyKind::Durability => "DURABILITY",
            QosPolicyKind::History => "HISTORY",
        }
    }
}

/// One endpoint's stated QoS policies, other than the depth.
///
/// Keyed exactly as [`DeclaredDepth`] is, so the two tables join without a
/// second naming convention -- and an endpoint appears here whenever it states
/// ANY of the three, which is not the same set as the endpoints that state a
/// depth. A field is `None` when that policy was not stated; that is the
/// `undeclared` half of the view, per policy and per kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredQosPolicies {
    pub kind: EntityKind,
    pub type_name: String,
    pub topic: String,
    pub reliability: Option<QoSReliabilityPolicy>,
    pub durability: Option<QoSDurabilityPolicy>,
    pub history: Option<QoSHistoryPolicy>,
}

impl DeclaredQosPolicies {
    /// The stated value of one policy, in the contract's own spelling.
    pub fn spelling(&self, policy: QosPolicyKind) -> Option<&'static str> {
        match policy {
            QosPolicyKind::Reliability => self.reliability.map(reliability_spelling),
            QosPolicyKind::Durability => self.durability.map(durability_spelling),
            QosPolicyKind::History => self.history.map(history_spelling),
        }
    }
}

/// Every stated non-depth QoS policy in an image, plus what stayed silent.
///
/// Same three-state discipline as [`DeclaredDepths`], and refused for the same
/// one reason: the inventory itself did not compose, so whole components are
/// missing and a missing row reads as "nobody declared this endpoint".
///
/// It does NOT refuse on `history = keep_all`. That refusal is per-FACT
/// (RFC-0100 D6) and the fact it kills is the DEPTH-derived one: a KEEP_ALL
/// queue has no static bound, so a depth beside it prices nothing. The
/// statement "this endpoint asked for KEEP_ALL" is itself perfectly well
/// declared, and a consumer -- an XRCE `STREAM_HISTORY`, a Cyclone resource
/// limit -- must be able to read it. Degrading it would be the global refusal
/// D6 exists to forbid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredQos {
    Resolved {
        /// Sorted by `(kind, type_name, topic)`, like the depth table.
        rows: Vec<DeclaredQosPolicies>,
        /// Per policy, per kind: endpoints of that kind that COULD have stated
        /// the policy and did not. `[(policy, kind) -> count]`, flattened into
        /// two small maps so the render stays a loop rather than nine fields.
        undeclared_subscriptions: [usize; 3],
        undeclared_publishers: [usize; 3],
    },
    Refused {
        reason: String,
    },
}

impl DeclaredQos {
    pub fn tag(&self) -> &'static str {
        match self {
            DeclaredQos::Resolved { .. } => "resolved",
            DeclaredQos::Refused { .. } => "refused",
        }
    }

    pub fn rows(&self) -> Option<&[DeclaredQosPolicies]> {
        match self {
            DeclaredQos::Resolved { rows, .. } => Some(rows),
            DeclaredQos::Refused { .. } => None,
        }
    }

    /// How many endpoints of `kind` did NOT state `policy`.
    ///
    /// `None` on a refusal, and that is the point: zero and "no answer" are the
    /// two things this view exists to keep apart.
    pub fn undeclared(&self, policy: QosPolicyKind, kind: EntityKind) -> Option<usize> {
        let DeclaredQos::Resolved {
            undeclared_subscriptions,
            undeclared_publishers,
            ..
        } = self
        else {
            return None;
        };
        let i = ALL_QOS_POLICY_KINDS.iter().position(|p| *p == policy)?;
        match kind {
            EntityKind::Subscription => Some(undeclared_subscriptions[i]),
            EntityKind::Publisher => Some(undeclared_publishers[i]),
            // Only the two topic kinds are counted. A service server states no
            // `reliability:` in any contract schema, so a count over it would
            // be a number nothing can ever move off its maximum -- issue 1227's
            // finding, which is why the depth counts are per kind too.
            _ => None,
        }
    }
}

/// phase-446 W4 -- the parameter every node carries without declaring it.
///
/// `Executor::seed_use_sim_time_default` declares `use_sim_time` on EVERY node
/// (phase-430 W2, as rclcpp does), so it takes a store slot per node whether
/// or not the contract names it. It is the only such name: nano-ros declares
/// no `start_type_description_service`, and it applies `qos_overrides.*` as
/// QoS rather than as parameters, so the other two names play_launch exempts
/// from its launch check claim no slot here. (checked with
/// `git grep -n 'start_type_description_service\|qos_overrides' -- packages/core packages/api`)
pub const SEEDED_PARAMETER: &str = "use_sim_time";

/// One parameter a node's contract declares (`nodes.<n>.params`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredParam {
    /// The node FQN, the key `contracts.node_params` uses.
    pub node: String,
    pub name: String,
    pub ty: ros_launch_manifest_model::ParamType,
}

impl DeclaredParam {
    /// `<node>:<param>:<type>` -- the one-token spelling that crosses into a
    /// build script. A ROS name holds no `:`, so the three fields split back
    /// apart unambiguously, and it has no space or `;` for a cmake list or a
    /// `cmake -E env` argument to mangle.
    pub fn token(&self) -> String {
        format!("{}:{}:{}", self.node, self.name, self.ty.as_str())
    }
}

/// What the contract says about the image's parameters, as a SIZING source.
///
/// Three states, and the first is not "zero parameters": a bringup whose
/// contract has no `params:` anywhere keeps every store knob on its default,
/// exactly as before the contract could say anything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ParamDeclarations {
    /// No node declares `params:`. Nobody said, so nothing is derived.
    #[default]
    Absent,
    /// Some nodes declare and some do not. The store holds every node's
    /// parameters, so a count over the nodes that declared is a count over a
    /// subset of the image -- the under-report `derive` refuses for entities.
    Refused { reason: String },
    /// Every node in the image declares. Sorted by `(node, name)`.
    Declared {
        nodes: Vec<String>,
        params: Vec<DeclaredParam>,
    },
}

/// A per-slot capacity the store needs for some declared type.
///
/// There is no `Derived(n)`: a capacity is a BOARD fact (an MCU and a PC want
/// different string lengths for the same node), so the contract can only say
/// whether one is needed at all. When it is, the board states the number and
/// the build refuses when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamCapacity {
    /// No declared parameter has a type that uses it.
    Unused,
    /// This declared parameter (the first, in `(node, name)` order) needs it.
    NeededBy(DeclaredParam),
}

/// The parameter-store knobs the declarations answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamStoreSizing {
    /// Parameters the contract declares, across every node.
    pub declared: usize,
    /// `NROS_MAX_PARAMETERS` -- per node, the declared names plus
    /// [`SEEDED_PARAMETER`] (once, even when the contract also names it: the
    /// seed steps aside for an application's own declaration and the two
    /// share a slot).
    pub max_parameters: usize,
    /// `NROS_MAX_PARAM_NAME_LEN` -- the longest of those names, in bytes.
    pub max_param_name_len: usize,
    /// `NROS_MAX_STRING_VALUE_LEN` -- `string` and `string_array`.
    pub string_value_len: ParamCapacity,
    /// `NROS_MAX_ARRAY_LEN` -- every array type except `byte_array`.
    pub array_len: ParamCapacity,
    /// `NROS_MAX_BYTE_ARRAY_LEN` -- `byte_array`.
    pub byte_array_len: ParamCapacity,
}

/// The three capacity knobs, the name each goes by, and which types use it.
/// ONE table, read by the derivation and by every renderer, so a knob cannot
/// be derived under one name and delivered under another.
pub const PARAM_CAPACITY_KNOBS: [&str; 3] = [
    "MAX_STRING_VALUE_LEN",
    "MAX_ARRAY_LEN",
    "MAX_BYTE_ARRAY_LEN",
];

impl ParamStoreSizing {
    /// `(knob suffix, capacity)` in [`PARAM_CAPACITY_KNOBS`] order.
    pub fn capacities(&self) -> [(&'static str, &ParamCapacity); 3] {
        [
            (PARAM_CAPACITY_KNOBS[0], &self.string_value_len),
            (PARAM_CAPACITY_KNOBS[1], &self.array_len),
            (PARAM_CAPACITY_KNOBS[2], &self.byte_array_len),
        ]
    }
}

/// phase-446 F3 -- what one node's declared parameters put through the six
/// parameter services, before any capacity is known.
///
/// The service buffer is bounded by the largest message those parameters can
/// produce, and every message is linear in two kinds of number: what the
/// CONTRACT decides (how many names, how long, which types -- this) and what
/// the BOARD decides (how long a string, an array or a description may be).
/// The board's numbers are resolved by `nros-params`' build script, where
/// the environment, Kconfig and `[knobs.params]` rungs meet, and nowhere
/// earlier, so the inventory carries this half and nros-node finishes the
/// bound against the resolved capacities (`param_service_bound` in
/// `parameter_services.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParamServiceShape {
    /// Parameters on the node: its declared names plus [`SEEDED_PARAMETER`].
    pub params: usize,
    /// The sum of those names' lengths, in bytes.
    pub name_bytes: usize,
    /// Distinct prefixes `list_parameters` reports: each name up to its last
    /// `.`, deduplicated, as `stream_list_parameters` computes them.
    pub prefixes: usize,
    /// The sum of those prefixes' lengths, in bytes.
    pub prefix_bytes: usize,
    /// `string` parameters.
    pub strings: usize,
    /// `byte_array` parameters.
    pub byte_arrays: usize,
    /// `bool_array` parameters.
    pub bool_arrays: usize,
    /// `integer_array` and `double_array` parameters: both are 8-byte words
    /// on the wire, so they cost the same.
    pub word_arrays: usize,
    /// `string_array` parameters.
    pub string_arrays: usize,
}

impl ParamServiceShape {
    /// `NROS_DECLARED_PARAM_SERVICE_SHAPE` -- one node per `,`, each the nine
    /// counts above joined by `:`, in field order. No space and no `;`, so a
    /// cmake list or a `cmake -E env` argument carries it intact. nros-node's
    /// build script parses exactly this and refuses anything else.
    pub fn token(shapes: &[ParamServiceShape]) -> String {
        shapes
            .iter()
            .map(|s| {
                format!(
                    "{}:{}:{}:{}:{}:{}:{}:{}:{}",
                    s.params,
                    s.name_bytes,
                    s.prefixes,
                    s.prefix_bytes,
                    s.strings,
                    s.byte_arrays,
                    s.bool_arrays,
                    s.word_arrays,
                    s.string_arrays
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl ParamDeclarations {
    pub fn tag(&self) -> &'static str {
        match self {
            ParamDeclarations::Absent => "absent",
            ParamDeclarations::Refused { .. } => "refused",
            ParamDeclarations::Declared { .. } => "declared",
        }
    }

    /// Read `contracts.node_params` against the nodes the model runs.
    pub fn from_model(model: &ros_launch_manifest_model::SystemModel) -> Self {
        let declared = &model.contracts.node_params;
        if declared.is_empty() {
            return ParamDeclarations::Absent;
        }
        let mut nodes: std::collections::BTreeSet<&String> = model.structure.nodes.keys().collect();
        nodes.extend(declared.keys());
        let silent: Vec<&str> = nodes
            .iter()
            .filter(|n| !declared.contains_key(n.as_str()))
            .map(|n| n.as_str())
            .collect();
        if !silent.is_empty() {
            return ParamDeclarations::Refused {
                reason: format!(
                    "{} of {} nodes in this image declare no `params:` in their contract: {}. \
                     The parameter store holds every node's parameters, and sizing it from \
                     the nodes that did declare would give the rest no slots. Declare \
                     `params:` on every node, or on none; until then the store knobs keep \
                     their configured values.",
                    silent.len(),
                    nodes.len(),
                    silent.join(", ")
                ),
            };
        }
        let params = declared
            .iter()
            .flat_map(|(node, ps)| {
                ps.iter().map(move |(name, c)| DeclaredParam {
                    node: node.clone(),
                    name: name.clone(),
                    ty: c.ty,
                })
            })
            .collect();
        ParamDeclarations::Declared {
            nodes: nodes.into_iter().cloned().collect(),
            params,
        }
    }

    /// phase-446 F3 -- each node's [`ParamServiceShape`], in node order, or
    /// `None` when nothing was declared (or refused). Same name set as
    /// [`Self::sizing`]: the declared names plus the seeded `use_sim_time`,
    /// once, typed `bool` unless the contract types it.
    pub fn service_shapes(&self) -> Option<Vec<ParamServiceShape>> {
        use ros_launch_manifest_model::ParamType as T;
        let ParamDeclarations::Declared { nodes, params } = self else {
            return None;
        };
        let shapes = nodes
            .iter()
            .map(|node| {
                let mut names: std::collections::BTreeMap<&str, T> = params
                    .iter()
                    .filter(|p| &p.node == node)
                    .map(|p| (p.name.as_str(), p.ty))
                    .collect();
                names.entry(SEEDED_PARAMETER).or_insert(T::Bool);
                let mut s = ParamServiceShape::default();
                let mut prefixes = std::collections::BTreeSet::new();
                for (name, ty) in &names {
                    s.params += 1;
                    s.name_bytes += name.len();
                    if let Some(dot) = name.rfind('.') {
                        prefixes.insert(&name[..dot]);
                    }
                    match ty {
                        T::String => s.strings += 1,
                        T::ByteArray => s.byte_arrays += 1,
                        T::BoolArray => s.bool_arrays += 1,
                        T::IntegerArray | T::DoubleArray => s.word_arrays += 1,
                        T::StringArray => s.string_arrays += 1,
                        T::Bool | T::Integer | T::Double => {}
                    }
                }
                s.prefixes = prefixes.len();
                s.prefix_bytes = prefixes.iter().map(|p| p.len()).sum();
                s
            })
            .collect();
        Some(shapes)
    }

    /// The store knobs, or `None` when nothing was declared (or refused).
    pub fn sizing(&self) -> Option<ParamStoreSizing> {
        use ros_launch_manifest_model::ParamType as T;
        let ParamDeclarations::Declared { nodes, params } = self else {
            return None;
        };
        let mut max_parameters = 0usize;
        let mut max_param_name_len = SEEDED_PARAMETER.len();
        for node in nodes {
            let names: std::collections::BTreeSet<&str> = params
                .iter()
                .filter(|p| &p.node == node)
                .map(|p| p.name.as_str())
                .chain([SEEDED_PARAMETER])
                .collect();
            max_parameters += names.len();
            for n in names {
                max_param_name_len = max_param_name_len.max(n.len());
            }
        }
        let first = |pred: fn(T) -> bool| -> ParamCapacity {
            params
                .iter()
                .find(|p| pred(p.ty))
                .map_or(ParamCapacity::Unused, |p| {
                    ParamCapacity::NeededBy(p.clone())
                })
        };
        Some(ParamStoreSizing {
            declared: params.len(),
            max_parameters,
            max_param_name_len,
            string_value_len: first(|t| matches!(t, T::String | T::StringArray)),
            array_len: first(|t| {
                matches!(
                    t,
                    T::BoolArray | T::IntegerArray | T::DoubleArray | T::StringArray
                )
            }),
            byte_array_len: first(|t| matches!(t, T::ByteArray)),
        })
    }
}

/// The DDS-mangled spelling a generated C++ message class carries as
/// `static constexpr const char* TYPE_NAME`.
///
/// `nav_msgs/msg/Odometry` -> `nav_msgs::msg::dds_::Odometry_`, which is
/// literally what `packs/cpp/message.hpp.jinja` emits
/// (`{{package_name}}::msg::dds_::{{message_name}}_`). Restated here rather
/// than shared because that emitter is a Tera template in a different crate;
/// `the_dds_spelling_matches_the_cpp_template` holds the two together.
///
/// A spelling that is already mangled (it contains `::`) passes through, so an
/// author who declares the C++ name gets what they wrote. Anything that is not
/// three `/`-separated segments also passes through unchanged: guessing at a
/// mangling for a shape the codegen does not emit would put a key in the table
/// that nothing can ever match, which is worse than no key.
pub fn dds_type_name(ros_name: &str) -> String {
    if ros_name.contains("::") {
        return ros_name.to_string();
    }
    let parts: Vec<&str> = ros_name.split('/').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        return ros_name.to_string();
    }
    format!("{}::{}::dds_::{}_", parts[0], parts[1], parts[2])
}

impl EntityInventory {
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            components: Vec::new(),
            infra: InfraServices::default(),
            params: ParamDeclarations::Absent,
            tiers: 0,
        }
    }

    /// Issue 1270 -- the runtime's own service families this inventory
    /// counts.
    pub fn infra(&self) -> InfraServices {
        self.infra
    }

    /// Issue 1270 -- state the families directly, for an inventory whose
    /// bringup facts arrive by some road other than [`Self::from_model`].
    pub fn set_infra(&mut self, infra: InfraServices) {
        self.infra = infra;
    }

    /// phase-446 W4 -- attach what the model's contract declares about
    /// parameters. Both composers call this with the same model (`nros build`'s
    /// resolve seed and the configure-time producer), so the fragment's bytes
    /// agree between them (issue 1228).
    pub fn set_param_declarations(&mut self, params: ParamDeclarations) {
        self.params = params;
    }

    pub fn param_declarations(&self) -> &ParamDeclarations {
        &self.params
    }

    /// Issue 1198 -- state the authored tier count for an inventory whose
    /// bringup facts arrive by some road other than [`Self::from_model`].
    pub fn set_tiers(&mut self, tiers: usize) {
        self.tiers = tiers;
    }

    /// Issue 1198 -- the authored tier count this inventory was given.
    pub fn tiers(&self) -> usize {
        self.tiers
    }

    /// phase-412 -- build the inventory from a resolved SystemModel's wiring
    /// instead of from `ENTITIES`.
    ///
    /// `structure.topics` carries, per topic, its message type and the endpoint
    /// refs on each side (`/node/endpoint`). That is exactly a per-node sub/pub
    /// set with types attached, which is what this inventory holds -- so the
    /// authored contract beside the launch file can replace the `ENTITIES` list
    /// duplicated in every component's `CMakeLists.txt`.
    ///
    /// # What each kind is read from
    ///
    /// | kind | model location |
    /// | --- | --- |
    /// | publisher, subscription | `structure.topics[*].{publishers,subscribers}` |
    /// | service server / client | `structure.services[*].{server,client}` |
    /// | action server / client | `structure.actions[*].{server,client}` |
    /// | timer | `contracts.node_paths[*]` with an EMPTY `input` |
    ///
    /// The timer row is the one that needs explaining. The model has no timer
    /// ENTITY, and for a while that was read as "the model cannot express a
    /// timer" -- it was the stated reason `ENTITIES` had to stay. It is not
    /// true. A node path is a `take -> publish` causal path, and the model's
    /// own definition of `PathContract::input` is "empty = periodic
    /// (timer-driven)", so a path with no inputs IS the periodic callback. The
    /// contract spells it `trigger: { timer: { rate_hz: N } }`, which the
    /// resolver flattens to a `node_paths` entry with no `input` key.
    ///
    /// The flattening loses the rest of the taxonomy: `once` and `spontaneous`
    /// triggers also arrive here as an empty `input`, and both are counted as
    /// a timer. That over-counts by one callback slot per such path, which is
    /// the SAFE direction -- an over-sized `MAX_CBS` costs bytes, an
    /// under-sized one halts entity creation at boot with a `BufferTooSmall`
    /// a dozen other paths also return. Recover the distinction by carrying
    /// the trigger onto `PathContract`, not by guessing here.
    ///
    /// Depth comes from `contracts.sub_endpoints[*].qos.depth` when the
    /// contract states one; an endpoint that states none yields `None`, never
    /// `0` -- a depth of zero is a QoS a subscriber cannot have, so it must
    /// never be the way "not declared" is spelled.
    ///
    /// # `name` is the TOPIC, not the endpoint ref (issue 1084)
    ///
    /// A contract addresses an endpoint as `/ns/node/<local name>` and wires it
    /// to an absolute topic under `structure.topics`. Those are two different
    /// strings -- `/mrm_handler/emergency_stop_status` against
    /// `/system/mrm/emergency_stop/status` -- and only the second one is what a
    /// `NROS_SUBSCRIBE` call site writes.
    ///
    /// [`EntityDecl::name`] is documented as "the topic / service / action
    /// name", and every consumer reads it that way: the declared-depth table is
    /// keyed `(type, topic)` and looked up at compile time with the call site's
    /// own literal. Recording the endpoint ref there produced a table that
    /// matched no call site in any image, which is the silent shape this
    /// campaign keeps paying for -- a check that is present, green and vacuous.
    /// The endpoint ref is still what the depth is LOOKED UP by; it is just not
    /// what the row is KEYED by.
    ///
    /// Returns `None` when the model describes no wiring, so a caller cannot
    /// mistake "nobody authored a contract" for "this image creates nothing".
    /// That distinction is the one this module exists to preserve.
    pub fn from_model(
        source: impl Into<String>,
        model: &ros_launch_manifest_model::SystemModel,
    ) -> Option<Self> {
        // `node_paths` counts here for the same reason the other three do: a
        // component whose only callback is a timer describes real wiring, and
        // an image made only of such components would otherwise read as "no
        // contract authored" and fall back to nothing.
        if model.structure.topics.is_empty()
            && model.structure.services.is_empty()
            && model.structure.actions.is_empty()
            && model.contracts.node_paths.is_empty()
        {
            return None;
        }

        // Endpoint refs are `/ns/node/endpoint`; the node FQN is everything but
        // the last segment. Group per node so each becomes one component row.
        fn node_of(ep: &str) -> String {
            ep.rsplit_once('/')
                .map(|(n, _)| n)
                .unwrap_or(ep)
                .to_string()
        }

        let mut per_node: std::collections::BTreeMap<String, Vec<EntityDecl>> =
            std::collections::BTreeMap::new();

        // A subscriber endpoint's declared history depth, when the contract
        // states one. `None` is "not declared" and stays `None`; see the note
        // on `EntityDecl::depth` for why it must never become `0`.
        let sub_depth_of = |ep: &str| -> Option<u32> {
            model
                .contracts
                .sub_endpoints
                .get(ep)
                .and_then(|c| c.qos.as_ref())
                .and_then(|q| q.depth)
        };

        // phase-454 W2 -- and the PUBLISHER's, from the other endpoint map.
        //
        // This read did not exist: every publisher row was built with
        // `depth: None`, so a contract stating `pub: { /chatter: { qos: {
        // depth: 8 } } }` reached `PubContract::qos.depth` in the model, was
        // never looked at, and the build saw an image whose publishers had
        // declared nothing. The grammar had already admitted the field --
        // `EntityKind::carries_qos_depth` returns true for a publisher and says
        // why ("it is a real QoS field and forbidding it here would make the
        // grammar say something false") -- so the declaration was legal to
        // write, legal to parse and dropped on the floor, which is the shape
        // this module's own rule calls a declaration the author believes they
        // made.
        let pub_depth_of = |ep: &str| -> Option<u32> {
            model
                .contracts
                .pub_endpoints
                .get(ep)
                .and_then(|c| c.qos.as_ref())
                .and_then(|q| q.depth)
        };

        // phase-454 W3 (issue 1256) -- the OTHER THREE policies, from whichever
        // endpoint map the kind lives in.
        //
        // `Qos` in the model carries `reliability`, `durability` and `history`
        // beside `depth`; this function read the fourth and dropped the other
        // three, which is the whole of issue 1256. They arrive as free-form
        // strings, so each is parsed through the ONE vocabulary
        // (`nros_orchestration_ir::qos_override`) the `qos_overrides.*` lowering
        // already uses.
        //
        // An UNRECOGNISED spelling parses to `None` here, which reads as "not
        // declared" -- and that would be the silent drop this module exists to
        // refuse. It cannot happen on the road a build takes: `nros ws
        // entity-inventory` runs `reject_unknown_qos_values` over the same model
        // BEFORE this, and that is where the error channel is (`from_model`
        // returns `Option` to say "no wiring described", which has no room for
        // "what you wrote is wrong" -- the same split issue 1084 made for
        // `depth: 0`). The refusal is checked, not assumed:
        // `an_unknown_qos_spelling_is_rejected_before_it_can_be_dropped` is the
        // test that binds the two halves.
        let sub_qos = |ep: &str| -> Option<&ros_launch_manifest_model::Qos> {
            model
                .contracts
                .sub_endpoints
                .get(ep)
                .and_then(|c| c.qos.as_ref())
        };
        let pub_qos = |ep: &str| -> Option<&ros_launch_manifest_model::Qos> {
            model
                .contracts
                .pub_endpoints
                .get(ep)
                .and_then(|c| c.qos.as_ref())
        };
        fn policies(
            q: Option<&ros_launch_manifest_model::Qos>,
        ) -> (
            Option<QoSReliabilityPolicy>,
            Option<QoSDurabilityPolicy>,
            Option<QoSHistoryPolicy>,
        ) {
            (
                q.and_then(|q| q.reliability.as_deref())
                    .and_then(parse_reliability),
                q.and_then(|q| q.durability.as_deref())
                    .and_then(parse_durability),
                q.and_then(|q| q.history.as_deref()).and_then(parse_history),
            )
        }

        // phase-454 W8 (RFC-0100 D9) -- the two RATES the queue-depth default
        // divides, read from the model's own conventions.
        //
        // PUBLISH rate: the channel's negotiated rate first
        // (`contracts.topics.<t>.rate_hz`), and the strongest promise its
        // publishers make second (`min_rate_hz`). The topic rate leads because
        // it is the CHANNEL's fact -- a topic with three publishers has one
        // arrival rate at the subscriber and three promises upstream -- and
        // `max` over the promises is the safe direction when there is no
        // channel rate: over-stating the arrival rate over-sizes the queue,
        // and under-stating it is the direction that ships a backlog.
        //
        // DRAIN rate: see `drain_rate_of` below. The model does not carry a
        // path's trigger (issue 1339), so this is a derivation from what the
        // node's timer paths PUBLISH, which is the same convention
        // `nros_orchestration_ir::mapper_input::pub_rate_hz` already uses to
        // give a periodic path its fire rate.
        let min_rate_of = |ep: &str| -> Option<f64> {
            model
                .contracts
                .pub_endpoints
                .get(ep)
                .and_then(|c| c.min_rate_hz)
        };
        let publish_rate_of =
            |topic: &str, wiring: &ros_launch_manifest_model::TopicWiring| -> Option<RateMilliHz> {
                if let Some(r) = model
                    .contracts
                    .topics
                    .get(topic)
                    .and_then(|t| t.rate_hz)
                    .and_then(RateMilliHz::from_hz)
                {
                    return Some(r);
                }
                wiring
                    .publishers
                    .iter()
                    .filter_map(|ep| min_rate_of(ep))
                    .filter_map(RateMilliHz::from_hz)
                    .max()
            };
        // A node's DRAIN rate: the rate of the timer paths it runs.
        //
        // A `node_paths` entry with an EMPTY `input` IS the periodic callback
        // -- the model's own definition, and the same test `from_model` already
        // uses to count a timer entity. Its RATE is not carried, so it is taken
        // from what the path publishes, exactly as `mapper_input` takes it.
        //
        // `min` over a node's timer paths, and the direction is the opposite of
        // the one above for the same reason: the SLOWEST drain is the one that
        // lets the most backlog accumulate, so it is the conservative
        // denominator. A node with one timer -- which is the shape the contract
        // describes when it says "drained batch-wise by the consuming timer" --
        // has one answer either way.
        //
        // Pairing a node's timer with a node's queue subscription is the layer
        // 2 rule, not an invention here: the resolver's own `queue-drain-rate`
        // check reads "node 'listener' timer path 'drain' rate_hz ... its
        // 'buffer: queue' subscriptions' producer rates".
        let mut drain_rate_by_node: std::collections::BTreeMap<String, RateMilliHz> =
            std::collections::BTreeMap::new();
        for (path_key, path) in &model.contracts.node_paths {
            if !path.input.is_empty() {
                continue;
            }
            let Some(rate) = path
                .output
                .iter()
                .filter_map(|ep| min_rate_of(ep))
                .filter_map(RateMilliHz::from_hz)
                .min()
            else {
                continue;
            };
            drain_rate_by_node
                .entry(node_of(path_key))
                .and_modify(|r| *r = (*r).min(rate))
                .or_insert(rate);
        }

        for (topic, wiring) in &model.structure.topics {
            let publish_rate = publish_rate_of(topic, wiring);
            for ep in &wiring.subscribers {
                let (reliability, durability, history) = policies(sub_qos(ep));
                per_node.entry(node_of(ep)).or_default().push(EntityDecl {
                    depth: sub_depth_of(ep),
                    reliability,
                    durability,
                    history,
                    // phase-454 W8 -- `buffer:` is NOT read here, because there
                    // is nothing to read. `SubContract` in the pinned
                    // `ros-launch-manifest` (v0.1.35) has no `buffer` field:
                    // the contract states it, the parser validates it, the
                    // resolver REASONS about it -- it emits a
                    // `[queue-drain-rate]` warning comparing exactly the two
                    // rates above -- and then writes a `sub_endpoints` entry
                    // without it. Issue 1339; the tripwire that goes red the
                    // day it lands is
                    // `tests/contract_queue_buffer_reaches_the_model.rs`.
                    //
                    // Left explicitly `None` and NOT defaulted to `Latest`
                    // even though `latest` is the schema's default for an
                    // absent key: "the author wrote latest" and "this reader
                    // cannot see what the author wrote" are different claims,
                    // and defaulting here would make the second one silently
                    // print as the first in every diagnostic below.
                    buffer: None,
                    publish_rate,
                    drain_rate: drain_rate_by_node.get(&node_of(ep)).copied(),
                    ..EntityDecl::bare(
                        EntityKind::Subscription,
                        Some(wiring.msg_type.clone()),
                        Some(topic.clone()),
                    )
                });
            }
            for ep in &wiring.publishers {
                let (reliability, durability, history) = policies(pub_qos(ep));
                per_node.entry(node_of(ep)).or_default().push(EntityDecl {
                    depth: pub_depth_of(ep),
                    reliability,
                    durability,
                    history,
                    ..EntityDecl::bare(
                        EntityKind::Publisher,
                        Some(wiring.msg_type.clone()),
                        Some(topic.clone()),
                    )
                });
            }
        }

        // Services and actions carry the same shape as topics -- a type plus
        // the endpoint refs on each side -- so they read the same way. The
        // model uses ONE `ServiceWiring` type for both, and the only thing
        // that distinguishes them is which map they came from.
        for (kinds, wirings) in [
            (
                (EntityKind::ServiceServer, EntityKind::ServiceClient),
                &model.structure.services,
            ),
            (
                (EntityKind::ActionServer, EntityKind::ActionClient),
                &model.structure.actions,
            ),
        ] {
            let (server_kind, client_kind) = kinds;
            for (service, wiring) in wirings {
                for (kind, eps) in [(server_kind, &wiring.server), (client_kind, &wiring.client)] {
                    for ep in eps {
                        per_node
                            .entry(node_of(ep))
                            .or_default()
                            .push(EntityDecl::bare(
                                kind,
                                Some(wiring.srv_type.clone()),
                                // The SERVICE / ACTION name, for the same reason
                                // the topic is used above: it is the string the
                                // call site writes.
                                Some(service.clone()),
                            ));
                    }
                }
            }
        }

        // Timers. The key is `<node FQN>/<path name>`, so `node_of` splits it
        // the same way an endpoint ref splits -- the path name takes the place
        // of the endpoint name.
        //
        // There is no type and no topic to record: a timer subscribes to
        // nothing. The NAME is kept because it is the only thing that
        // distinguishes two timers on one node, and a consumer rendering the
        // inventory should be able to say which path it is.
        for (path_key, path) in &model.contracts.node_paths {
            if !path.input.is_empty() {
                continue;
            }
            per_node
                .entry(node_of(path_key))
                .or_default()
                .push(EntityDecl::bare(
                    EntityKind::Timer,
                    None,
                    Some(path_key.clone()),
                ));
        }

        let mut inv = Self::new(source);
        // Issue 1270 -- the families the bringup declares ride along, so the
        // session pools count the servers the runtime creates for them.
        inv.infra = InfraServices::from_model(model);
        // Issue 1198 -- the SCHEDULING declaration rides along too. It is not
        // an entity fact and is deliberately kept apart from `infra`: the
        // service families are what the runtime creates FOR the image, while
        // this is what the integrator authored about how it runs (RFC-0016
        // tiers, `[tiers.*]` in `system.toml`).
        inv.tiers = model.execution.tiers.len();
        for (node_fqn, entities) in per_node {
            let component = node_fqn.rsplit('/').next().unwrap_or(&node_fqn).to_string();
            inv.insert(ComponentEntities {
                // The model names nodes, not ament packages. A node FQN is the
                // stable identity here, and stating it as the package rather
                // than inventing one keeps the provenance line honest about
                // where the row came from.
                pkg: node_fqn.clone(),
                component,
                class: String::new(),
                declaration: Declaration::Stated(entities),
            });
        }
        Some(inv)
    }

    /// phase-412 -- combine a declaration-derived inventory with a
    /// model-derived one, PER COMPONENT and PER KIND, taking whichever source
    /// says more.
    ///
    /// Neither source is complete, which is why this is a max and not a choice
    /// -- the same rule, for the same reason, that
    /// `model_ingest::count_callbacks_with_metadata` applies one layer up:
    ///
    /// * The MODEL has no timer entity. The island runs four timers and the
    ///   contract cannot express one, so a model-only `MAX_CBS` is short by
    ///   four and short halts the board.
    /// * The DECLARATION is hand-written. mrm_handler's said six subscriptions
    ///   where the code creates seven, and nothing compared the two; the model
    ///   gets its seven from an authored contract instead.
    ///
    /// Per KIND rather than per component total, because the two blind spots
    /// are in different kinds: taking a whole-component max would let the
    /// declaration's four timers hide the model's extra subscription, or the
    /// reverse. Per kind, each source can only ever raise the answer.
    ///
    /// UNION would double-count: both sources describe the same subscriptions,
    /// so adding them sizes every pool at twice the truth.
    ///
    /// The join key is the COMPONENT name -- `nano_ros_node_register(NAME ...)`
    /// on one side and the launch `exec` on the other, which RFC-0057 already
    /// requires to be the same string. A component in one source and not the
    /// other is carried through unchanged rather than dropped.
    #[must_use]
    pub fn merged_per_kind_max(&self, model: &EntityInventory) -> EntityInventory {
        use std::collections::BTreeMap;

        fn by_kind(d: &Declaration) -> BTreeMap<EntityKind, Vec<EntityDecl>> {
            let mut m: BTreeMap<EntityKind, Vec<EntityDecl>> = BTreeMap::new();
            for e in d.entities() {
                m.entry(e.kind).or_default().push(e.clone());
            }
            m
        }

        let model_rows: BTreeMap<&str, &ComponentEntities> = model
            .components
            .iter()
            .map(|c| (c.component.as_str(), c))
            .collect();

        let mut out = Self::new(format!("{} + {}", self.source, model.source));
        // Issue 1270 -- metadata carries no bringup features, so the model's
        // declaration is what survives; a union, never a max of two counts.
        out.infra = self.infra.union(model.infra);
        // Issue 1198 -- the tier table is the model's alone (a probe sees no
        // `system.toml`), so this is a max for the same reason `infra` is a
        // union: whichever side saw the declaration is the one that knows.
        out.tiers = self.tiers.max(model.tiers);
        let mut seen: Vec<&str> = Vec::new();

        for decl_row in &self.components {
            let Some(model_row) = model_rows.get(decl_row.component.as_str()) else {
                out.insert(decl_row.clone());
                continue;
            };
            seen.push(decl_row.component.as_str());

            // A component the declaration says nothing about is NOT a zero, so
            // the model simply stands. `Declaration::None` IS a zero and the
            // max still holds.
            let decl_kinds = by_kind(&decl_row.declaration);
            let model_kinds = by_kind(&model_row.declaration);

            let mut merged: Vec<EntityDecl> = Vec::new();
            let mut kinds: Vec<EntityKind> = decl_kinds
                .keys()
                .chain(model_kinds.keys())
                .copied()
                .collect();
            kinds.sort_by_key(|k| k.tag());
            kinds.dedup();
            for k in kinds {
                let d = decl_kinds.get(&k).map(Vec::as_slice).unwrap_or(&[]);
                let m = model_kinds.get(&k).map(Vec::as_slice).unwrap_or(&[]);
                // The LONGER list wins whole, so the winning source's types and
                // topic names survive intact rather than being spliced.
                merged.extend_from_slice(if m.len() > d.len() { m } else { d });
            }

            out.insert(ComponentEntities {
                pkg: decl_row.pkg.clone(),
                component: decl_row.component.clone(),
                class: decl_row.class.clone(),
                declaration: if merged.is_empty() {
                    decl_row.declaration.clone()
                } else {
                    Declaration::Stated(merged)
                },
            });
        }

        for (name, row) in &model_rows {
            if !seen.contains(name) {
                out.insert((*row).clone());
            }
        }
        // phase-446 W4 -- parameter declarations exist only in the MODEL (a
        // metadata declaration has no way to state one), so the model's
        // answer stands whenever it has one.
        out.params = match &model.params {
            ParamDeclarations::Absent => self.params.clone(),
            p => p.clone(),
        };
        out
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
                    // issue 1033 -- this used to say "Add `ENTITIES ...` to each
                    // `nano_ros_node_register()`", which phase-412 turned into a
                    // FATAL_ERROR: the one remedy the refusal named was the one
                    // thing the caller could not do. A diagnostic that survives the
                    // mechanism it describes aims the next reader at a wall, and
                    // this one did it for every standalone image in the tree.
                    "{} of {} components in this image declare no entities:\n{block}\n\
                     Deriving over only the components that did would publish a slot count \
                     smaller than the image needs, and a short NROS_EXECUTOR_MAX_CBS fails \
                     entity creation at boot. State what each component creates in the \
                     contract sidecar beside the launch file that runs it \
                     (<bringup>/launch/<stem>.contract.yaml), which reaches this through the \
                     SystemModel. An image that runs from no launch file -- a standalone \
                     Zephyr application, say -- reaches no contract, so it keeps its \
                     CONFIGURED pool knobs and must state the ones its RMW session sizes \
                     (see issue 1033).",
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
        let mut heavy_slots = 0usize;
        let mut entity_total = 0usize;
        // Issue 1130 -- per COMPONENT, per KIND, over the five kinds a cell
        // registers. Counted in this loop, beside the per-kind totals, so the
        // cell bound and every other knob read one pass over one declaration.
        let mut max_cell_entities = 0usize;
        for c in self.components() {
            let mut slots = 0usize;
            let mut count = 0usize;
            let mut cell: BTreeMap<&'static str, usize> = BTreeMap::new();
            for e in c.declaration.entities() {
                *per_kind.entry(e.kind.tag()).or_insert(0) += 1;
                slots += e.kind.callback_slots();
                if matches!(e.kind, EntityKind::ActionClient | EntityKind::ActionServer) {
                    heavy_slots += e.kind.callback_slots();
                }
                if matches!(
                    e.kind,
                    EntityKind::Publisher
                        | EntityKind::ServiceServer
                        | EntityKind::ServiceClient
                        | EntityKind::ActionClient
                        | EntityKind::ActionServer
                ) {
                    *cell.entry(e.kind.tag()).or_insert(0) += 1;
                }
                count += 1;
            }
            max_cell_entities = max_cell_entities.max(cell.values().copied().max().unwrap_or(0));
            max_cbs += slots;
            entity_total += count;
            per_component.push((c.pkg.clone(), c.component.clone(), count, slots));
        }

        // phase-412 W1. A declared action is ONE entity that costs SEVERAL
        // session slots, so the session pools are the per-kind count PLUS the
        // multipliers held beside the calls that decide them. Reading the raw
        // count would size every action-carrying image short.
        // These three numbers are the image's DEMAND, and they are published
        // unfloored -- a declaration with no subscriptions demands 0. Issue
        // 1015 floored them HERE first, and that was the wrong layer: whether
        // zero is a legal pool size is a property of the CONSUMER's storage,
        // not of the count, and this one derivation feeds two consumers that
        // answer it differently.
        //
        //   zenoh   `queryable_entry_t queryables[ZPICO_MAX_QUERYABLES]` and
        //           its two siblings are fixed C arrays, and a derived 0 gave
        //           a board that transmitted NOTHING in 15 s -- no panic, no
        //           log, core in WFI (issue 1015, measured on the reference
        //           island). Those knobs carry a floor of ONE, applied where
        //           they are named: [`c_array_pool_floor`] on the cargo lane
        //           and `_nros_c_array_pool_floor` in `nros_cargo_build.cmake`
        //           for the CMake one, with `#if ... < 1 / #error` beside the
        //           arrays in `zpico.c` as the backstop that binds a producer
        //           neither of those reaches.
        //
        //   xrce    the same derived numbers reach `NROS_XRCE_MAX_SUBSCRIBERS`
        //           and `NROS_XRCE_MAX_SERVICE_SERVERS`, where ZERO IS THE
        //           ANSWER and is worth 33,296 and 4,384 bytes of heap per
        //           slot (issue 1033, measured from the zephyr cpp listener's
        //           DWARF). Its `build.rs` lowered those minima from 1 to 0 on
        //           purpose; a floor here silently defeated that the day
        //           before it landed, and no gate could see it, because the
        //           number was derived correctly and delivered faithfully.
        //
        // So: demand here, floors at the pools. `check-c-array-pool-floors`
        // holds both ends together.
        let n = |tag: &str| per_kind.get(tag).copied().unwrap_or(0);
        let max_subscribers = n(EntityKind::Subscription.tag())
            + n(EntityKind::ActionClient.tag()) * ACTION_CLIENT_SUBSCRIPTIONS;
        let max_publishers = n(EntityKind::Publisher.tag())
            + n(EntityKind::ActionServer.tag()) * ACTION_SERVER_PUBLISHERS;
        // Issue 1270 -- plus the servers the runtime creates for the families
        // the bringup declares: six per node for `param_services`, five once
        // for `lifecycle`. Not a guess: the bringup states the feature, and
        // the multipliers are held to the creation calls by
        // check-infra-queryable-counts.
        let components = self.components.len();
        let infra_queryables = self.infra.queryables(components);
        let param_service_nodes = self.infra.param_nodes(components);
        let max_queryables = n(EntityKind::ServiceServer.tag())
            + n(EntityKind::ActionServer.tag()) * ACTION_SERVER_QUERYABLES
            + infra_queryables;
        let max_nodes = self.components().len();
        // Issue 1198 -- slot 0 is RESERVED for the default Fifo context
        // (`create_sched_context` searches `1..MAX_SC`), so the demand is one
        // plus whatever the schedule creates. See `DerivedEntityKnobs::max_sc`
        // for why the second term is the larger of the authored tier count and
        // the node count rather than the tier count alone.
        let max_sc = 1 + self.tiers.max(max_nodes);

        // phase-412 W2 -- the liveliness pool. Terms and their call sites are
        // on the field; every one is a count this derivation already made.
        let service_clients = n(EntityKind::ServiceClient.tag())
            + n(EntityKind::ActionClient.tag()) * ACTION_CLIENT_SERVICE_CLIENTS;
        let node_tokens = max_nodes.max(param_service_nodes)
            + PRIMARY_NODE_LIVELINESS_TOKENS
            + usize::from(self.infra.lifecycle);
        let max_liveliness =
            node_tokens + max_publishers + max_subscribers + max_queryables + service_clients;

        Derivation::Derived(Box::new(DerivedEntityKnobs {
            max_cbs,
            heavy_slots,
            entity_total,
            max_subscribers,
            max_publishers,
            max_queryables,
            infra_queryables,
            param_service_nodes,
            max_nodes,
            max_liveliness,
            max_cell_entities,
            max_sc,
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

    /// Every declared QoS history depth in this image (phase-403 step 2).
    ///
    /// REFUSES on exactly one condition -- the image's own composition refused,
    /// because some component declared no `ENTITIES` at all. Its endpoints are
    /// then unknown, so a depth table composed over the components that DID
    /// answer would be missing rows that a compile-time check would then read
    /// as "nobody declared this topic" and let through.
    ///
    /// It does NOT refuse on a missing depth. An endpoint that states none is
    /// counted in `undeclared` and is simply absent from the table: that image
    /// has not opted in, which is not an error. The arena (step 3) refuses on a
    /// non-zero `undeclared` the way W8 refuses on an unbounded type; the
    /// compile-time check sees no row and asserts nothing.
    ///
    /// # …and it DOES refuse on `history = keep_all` (phase-454 W3, RFC-0100 D6)
    ///
    /// This is the one live defect behind that wave, and it is the only trigger
    /// in the whole sizing model that could ship a buffer that is too SMALL
    /// rather than too large.
    ///
    /// `KEEP_LAST(n)` bounds a queue at `n` samples. `KEEP_ALL` bounds it at
    /// nothing -- DDS keeps every sample the resource limits allow, and no
    /// number in a contract says how many that is. A `keep_all` endpoint that
    /// also states `depth: 1` was priced at 1 here: the depth reached the table,
    /// the history did not, and the arena budgeted one sample for a queue with
    /// no bound. That is a silent UNDER-size, and it lands as
    /// `NodeError::BufferTooSmall` at a registration the arena oracle passed.
    ///
    /// So the DEPTH-derived facts refuse, and they refuse whether or not a depth
    /// was stated beside the `keep_all` -- a stated depth is not a cap on a
    /// KEEP_ALL queue, it is a number that has stopped meaning what it says.
    ///
    /// The refusal is PER FACT and stops here. A `keep_all` subscription says
    /// nothing about Cyclone's type table, about the entity counts, or about the
    /// parameter store, and D6 forbids degrading them: *"a `keep_all`
    /// subscription says nothing about Cyclone's type table, and a global
    /// refusal would degrade it anyway."* [`EntityInventory::declared_qos`] keeps
    /// resolving too, including the `keep_all` row itself -- the statement is
    /// well declared, it is only the DEPTH arithmetic that has no answer.
    ///
    /// And it does not fall back to a number. RFC-0100 D6: a refused fact never
    /// silently widens its basis, because *"that publishes the wrong row while
    /// every status still reads 'derived', which is the shape that looks like it
    /// worked."*
    pub fn declared_depths(&self) -> DeclaredDepths {
        if let Derivation::Refused { reason } = self.derive() {
            return DeclaredDepths::Refused {
                reason: format!(
                    "the entity inventory itself did not compose, so the declared-depth table \
                     would be missing whole components -- and a missing row reads as \"nobody \
                     declared this endpoint\", which is the one thing the table must never \
                     say wrongly:\n{reason}"
                ),
            };
        }

        // phase-454 W3 -- KEEP_ALL first, because every number below it would be
        // an answer to a question that has none. Named endpoints, not a count: a
        // refusal a user cannot act on is a refusal they work around.
        let keep_all: Vec<String> = self
            .components()
            .iter()
            .flat_map(|c| {
                c.declaration
                    .entities()
                    .iter()
                    .filter(|e| e.history == Some(QoSHistoryPolicy::KeepAll))
                    .map(move |e| {
                        format!(
                            "    {}::{} declares a `{}` on `{}`{}",
                            c.pkg,
                            c.component,
                            e.kind.tag(),
                            e.name.as_deref().unwrap_or("<unnamed>"),
                            match e.depth {
                                Some(d) => format!(" with `depth: {d}` beside it"),
                                None => String::new(),
                            }
                        )
                    })
            })
            .collect();
        if !keep_all.is_empty() {
            return DeclaredDepths::Refused {
                reason: format!(
                    "{} endpoint(s) declare `history: keep_all`, which has NO STATIC BOUND -- \
                     KEEP_ALL keeps every sample the resource limits allow, and no number in a \
                     contract says how many that is:\n{}\nA depth stated beside KEEP_ALL is not \
                     a cap on the queue, so pricing from it would UNDER-size the buffer and ship \
                     `BufferTooSmall` at a registration this table had passed. Declare \
                     `history: keep_last` with the `depth:` you mean, which is the pair every \
                     size consumer here can derive from.",
                    keep_all.len(),
                    keep_all.join("\n")
                ),
            };
        }

        let mut rows: Vec<DeclaredDepth> = Vec::new();
        let mut undeclared = 0usize;
        let mut undeclared_subscriptions = 0usize;
        let mut undeclared_publishers = 0usize;
        for c in self.components() {
            for e in c.declaration.entities() {
                if !e.kind.carries_qos_depth() {
                    continue;
                }
                // phase-454 W8 -- RFC-0049's ladder, in order. A STATED depth
                // is taken first and nothing below it runs; only where nobody
                // stated one does the rate derivation get a turn, and only for
                // an endpoint that declared `buffer: queue`. Every other
                // endpoint takes the `None` arm exactly as it did before this
                // wave, which is what makes an image with no queue endpoint
                // byte-identical (`depth_default` returns `NotAQueue` for a
                // `latest` endpoint and for one that stated no discipline).
                //
                // `keep_all` never reaches here: the refusal above returns
                // first, for the whole table. That ordering is the W3 contract
                // this wave has to honour -- a KEEP_ALL queue has no static
                // bound, and deriving a number for one would be the same
                // UNDER-size W3 refuses, arrived at by arithmetic instead of by
                // believing a stated depth.
                let resolved = match e.depth {
                    Some(depth) => Some((depth, DepthSource::Stated)),
                    None => depth_default(e.buffer, None, e.publish_rate, e.drain_rate)
                        .ok()
                        .map(|depth| (depth, DepthSource::DerivedFromRates)),
                };
                match (resolved, &e.type_name, &e.name) {
                    (Some((depth, source)), Some(t), Some(n)) => rows.push(DeclaredDepth {
                        kind: e.kind,
                        type_name: t.clone(),
                        topic: n.clone(),
                        depth,
                        source,
                    }),
                    // A depth with no type or no topic cannot be JOINED to
                    // anything -- the table is keyed `(type, topic)` and the
                    // arena charges a buffer per typed endpoint. It counts as
                    // undeclared rather than being dropped silently, so the
                    // count stays the honest "endpoints this image cannot size".
                    (Some(_), _, _) | (None, _, _) => {
                        undeclared += 1;
                        match e.kind {
                            EntityKind::Subscription => undeclared_subscriptions += 1,
                            EntityKind::Publisher => undeclared_publishers += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
        // Sorted by `(kind, type, topic)`, not by `(type, topic)`: a publisher
        // and a subscription on ONE topic are now two rows that agree on both
        // of the old keys, so the old ordering left their relative position to
        // the sort's stability and the component iteration order -- which is a
        // byte-unstable artifact for the two consumers that split the table by
        // kind. The kind leads so each consumer's slice is contiguous.
        rows.sort_by(|a, b| {
            (a.kind.tag(), &a.type_name, &a.topic).cmp(&(b.kind.tag(), &b.type_name, &b.topic))
        });
        DeclaredDepths::Resolved {
            rows,
            undeclared,
            undeclared_subscriptions,
            undeclared_publishers,
        }
    }

    /// Every endpoint's queue-depth default, DERIVED OR NOT -- phase-454 W8.
    ///
    /// Sibling of [`Self::declared_depths`] and deliberately not folded into
    /// it, for the reason [`Self::declared_qos`] is separate: this view's
    /// population is different. The depth table carries the endpoints that HAVE
    /// a depth; this one carries the endpoints that could have been defaulted
    /// and says, for each, what happened -- which is the only place an author
    /// can read WHY an endpoint got no default.
    ///
    /// That "why" is the whole of RFC-0100 D9's acceptance 3. Where either rate
    /// is absent there is no default, and the absence has to be VISIBLE: a
    /// derivation that silently declines is indistinguishable from one that was
    /// never asked, which is the shape this campaign keeps paying for.
    ///
    /// Restricted to SUBSCRIPTIONS. `buffer:` is defined only on a subscriber
    /// endpoint (`parse_buffer` in the manifest is a parse-time error anywhere
    /// else), and a publisher has no queue a timer drains.
    pub fn queue_depth_defaults(&self) -> Vec<QueueDepthDefault> {
        let mut out = Vec::new();
        for c in self.components() {
            for e in c.declaration.entities() {
                if e.kind != EntityKind::Subscription {
                    continue;
                }
                let Some(topic) = e.name.as_deref() else {
                    continue;
                };
                out.push(QueueDepthDefault {
                    topic: topic.to_string(),
                    type_name: e.type_name.clone(),
                    buffer: e.buffer,
                    stated_depth: e.depth,
                    publish_rate: e.publish_rate,
                    drain_rate: e.drain_rate,
                    outcome: depth_default(e.buffer, e.depth, e.publish_rate, e.drain_rate),
                });
            }
        }
        out.sort_by(|a, b| a.topic.cmp(&b.topic));
        out
    }

    /// The two `buffer:` diagnostics -- phase-454 W8, RFC-0100 D9.
    ///
    /// WARNINGS, never errors. See [`BufferDiagnostic`]: both shapes are legal
    /// and a legitimate image can want either, so this returns prose for a
    /// caller to print and has no error channel at all. Making either fatal
    /// would be the build deciding an application question.
    ///
    /// Empty for every image that states no `buffer:` -- which is every image
    /// in this tree today, because the SystemModel does not carry the key
    /// (issue 1339). That is why nothing in the warning stream moves and why
    /// the byte-identical proof holds.
    pub fn buffer_diagnostics(&self) -> Vec<BufferDiagnostic> {
        let mut out = Vec::new();
        for c in self.components() {
            for e in c.declaration.entities() {
                if !e.kind.carries_qos_depth() {
                    continue;
                }
                let Some(topic) = e.name.as_deref() else {
                    continue;
                };
                if let Some(d) = diagnose(e.kind.tag(), topic, e.buffer, e.depth) {
                    out.push(d);
                }
            }
        }
        out
    }

    /// The other three QoS policies -- phase-454 W3, issue 1256.
    ///
    /// Sibling of [`Self::declared_depths`], and deliberately a SEPARATE view
    /// rather than three more columns on that one. Two reasons:
    ///
    /// 1. **Its population differs.** A depth row exists only where a depth was
    ///    stated; an endpoint can state `reliability: best_effort` and no depth
    ///    at all, and it has to appear somewhere.
    /// 2. **Its status differs, and that is the whole of RFC-0100 D6.** A
    ///    `keep_all` endpoint makes the depth table REFUSE and leaves this one
    ///    resolved, because the policy is well declared and only the depth
    ///    arithmetic has no answer. Folding them into one view would make the
    ///    per-fact refusal impossible to express.
    ///
    /// Refuses on exactly the one condition the depth table refuses on and no
    /// other: the inventory itself did not compose, so whole components are
    /// missing and an absent row would read as "nobody declared this endpoint".
    pub fn declared_qos(&self) -> DeclaredQos {
        if let Derivation::Refused { reason } = self.derive() {
            return DeclaredQos::Refused {
                reason: format!(
                    "the entity inventory itself did not compose, so the declared-QoS table \
                     would be missing whole components -- and a missing row reads as \"nobody \
                     declared this endpoint\", which is the one thing the table must never \
                     say wrongly:\n{reason}"
                ),
            };
        }

        let mut rows: Vec<DeclaredQosPolicies> = Vec::new();
        let mut undeclared_subscriptions = [0usize; 3];
        let mut undeclared_publishers = [0usize; 3];
        for c in self.components() {
            for e in c.declaration.entities() {
                // The same population the depth view walks: a timer and a guard
                // condition carry no QoS at all, so counting them as "did not
                // state a reliability" would pin every consumer on its worst
                // case for endpoints that cannot ever state one.
                if !e.kind.carries_qos_depth() {
                    continue;
                }
                let counts = match e.kind {
                    EntityKind::Subscription => Some(&mut undeclared_subscriptions),
                    EntityKind::Publisher => Some(&mut undeclared_publishers),
                    // Services and actions carry QoS in ROS, but no contract
                    // schema states one for them and `from_model` has no map to
                    // read it from. Counting them would make every count
                    // permanently non-zero -- issue 1227's defect exactly.
                    _ => None,
                };
                if let Some(counts) = counts {
                    for (i, policy) in ALL_QOS_POLICY_KINDS.iter().enumerate() {
                        let stated = match policy {
                            QosPolicyKind::Reliability => e.reliability.is_some(),
                            QosPolicyKind::Durability => e.durability.is_some(),
                            QosPolicyKind::History => e.history.is_some(),
                        };
                        // A policy stated on an endpoint with no type or no
                        // topic cannot be JOINED to anything, so it counts as
                        // undeclared rather than being dropped -- the rule the
                        // depth view applies to the same shape.
                        if !stated || e.type_name.is_none() || e.name.is_none() {
                            counts[i] += 1;
                        }
                    }
                }
                let (Some(t), Some(n)) = (&e.type_name, &e.name) else {
                    continue;
                };
                if e.reliability.is_none() && e.durability.is_none() && e.history.is_none() {
                    continue;
                }
                rows.push(DeclaredQosPolicies {
                    kind: e.kind,
                    type_name: t.clone(),
                    topic: n.clone(),
                    reliability: e.reliability,
                    durability: e.durability,
                    history: e.history,
                });
            }
        }
        rows.sort_by(|a, b| {
            (a.kind.tag(), &a.type_name, &a.topic).cmp(&(b.kind.tag(), &b.type_name, &b.topic))
        });
        DeclaredQos::Resolved {
            rows,
            undeclared_subscriptions,
            undeclared_publishers,
        }
    }

    /// The C and C++ compile-time table: `nros_declared_qos_generated.h`.
    ///
    /// TWO X-macro lists, the same rows in both (phase-454 W10):
    /// `NROS_DECLARED_QOS_ROWS`, read by `nros/declared_qos.hpp`, and
    /// `NROS_DECLARED_QOS_ROWS_Q`, read by `nros/declared_qos.h`. The second
    /// exists because C has no `constexpr`, so a C lookup is built by the
    /// PREPROCESSOR and the queried `(type, topic)` has to travel through the
    /// list to reach each row -- see the comment beside its emission below.
    ///
    /// SUBSCRIPTIONS only, and that is the scope of the consumer rather than a
    /// shortcut. `NROS_SUBSCRIBE` is the one macro that asserts against this
    /// table, and keying `(type, topic)` means a publisher and a subscription
    /// on the same pair would be two rows with one key. When the publish side
    /// grows a check the row gains a kind column; until then a row nothing can
    /// consult is a row that can silently be wrong.
    ///
    /// Emitted as an X-MACRO rather than a C++ array so the file is pure
    /// preprocessor: it can then be included in any order, from any language
    /// mode, and `nros/declared_qos.hpp` owns the one definition of the table's
    /// TYPE. A generated header that also declared the struct would have to be
    /// included at exactly one point of exactly one header.
    ///
    /// Both spellings of the type are emitted per row -- the ROS
    /// `pkg/msg/Name` the declaration used and the DDS-mangled
    /// `pkg::msg::dds_::Name_` a generated C++ class carries. The lookup is a
    /// compile-time linear scan, so a second row costs nothing at runtime, and
    /// which spelling a message class carries is a property of the CODEGEN that
    /// produced it, not something this file should have to predict.
    pub fn to_declared_qos_header(&self) -> String {
        let mut s = String::new();
        // Written line by line, NOT as one `\`-continued literal: Rust strips
        // the leading whitespace after a line continuation, which silently ate
        // the ` ` before every `*` and produced a comment block no C formatter
        // would accept.
        for line in [
            "/* GENERATED by `nros ws entity-inventory` (phase-403 step 2). Do not edit.",
            " *",
            " * The QoS history DEPTH each subscription was DECLARED with, in",
            " * `nano_ros_node_register(... ENTITIES sub:<type>:<topic>@depth=N ...)`.",
            " * `nros/declared_qos.hpp` expands this into a `constexpr` table and",
            " * `NROS_SUBSCRIBE` static_asserts the QoS it is handed against it, so a",
            " * declaration and an implementation that disagree fail the BUILD naming",
            " * the topic and both numbers.",
            " *",
            " * An ABSENT row is not depth 0 and not depth 10: it is \"nobody declared",
            " * this endpoint\", and nothing asserts against it.",
            " *",
        ] {
            s.push_str(line);
            s.push('\n');
        }
        s.push_str(&format!(
            " * Source: {}\n */\n",
            self.source.replace("*/", "*_/")
        ));
        s.push_str("#ifndef NROS_DECLARED_QOS_GENERATED_H\n");
        s.push_str("#define NROS_DECLARED_QOS_GENERATED_H\n\n");

        let depths = self.declared_depths();
        match &depths {
            DeclaredDepths::Refused { reason } => {
                // A refusal emits NO rows and says so in the file, on the rule
                // the rest of this module holds: a consumer reads a table this
                // module built or reads nothing. `NROS_DECLARED_QOS_ROWS` stays
                // undefined, so `declared_qos.hpp` compiles an empty table and
                // every call site keeps working unchecked.
                s.push_str("/* NO TABLE. The entity inventory refused to compose:\n *   ");
                s.push_str(&reason.replace('\n', "\n *   ").replace("*/", "*_/"));
                s.push_str("\n */\n");
                s.push_str("#define NROS_DECLARED_QOS_STATUS \"refused\"\n");
            }
            DeclaredDepths::Resolved {
                rows, undeclared, ..
            } => {
                // phase-454 W8 -- STATED rows only. A derived default sizes the
                // arena and must never become a `static_assert`: an image that
                // declared no depth would then have to spell this CLI's
                // arithmetic at every `NROS_SUBSCRIBE` or fail to compile, and
                // moving the margin by one slot would break every such image.
                // See [`DepthSource`], which exists for exactly this split.
                let subs: Vec<&DeclaredDepth> = rows
                    .iter()
                    .filter(|r| r.kind == EntityKind::Subscription)
                    .filter(|r| r.source == DepthSource::Stated)
                    .collect();
                s.push_str("#define NROS_DECLARED_QOS_STATUS \"resolved\"\n");
                s.push_str(&format!(
                    "/* {} of this image's depth-carrying endpoints declared no depth. */\n",
                    undeclared
                ));
                s.push_str(&format!(
                    "#define NROS_DECLARED_QOS_UNDECLARED_COUNT {undeclared}\n\n"
                ));
                if subs.is_empty() {
                    s.push_str(
                        "/* No subscription in this image declared a depth, so there is no\n \
                         * table to assert against. NROS_DECLARED_QOS_ROWS stays undefined\n \
                         * -- an empty list and \"nobody said\" must not look alike. */\n",
                    );
                } else {
                    s.push_str(
                        "/* X-macro. `nros/declared_qos.hpp` defines NROS_DECLARED_QOS_ROW\n \
                         * and expands this; nothing else may. */\n",
                    );
                    s.push_str("#define NROS_DECLARED_QOS_ROWS \\\n");
                    for r in &subs {
                        let dds = dds_type_name(&r.type_name);
                        s.push_str(&format!(
                            "    NROS_DECLARED_QOS_ROW(\"{}\", \"{}\", {}) \\\n",
                            c_escape(&dds),
                            c_escape(&r.topic),
                            r.depth
                        ));
                        if dds != r.type_name {
                            s.push_str(&format!(
                                "    NROS_DECLARED_QOS_ROW(\"{}\", \"{}\", {}) \\\n",
                                c_escape(&r.type_name),
                                c_escape(&r.topic),
                                r.depth
                            ));
                        }
                    }
                    s.push_str("    /* end */\n");
                    // phase-454 W10 -- the SAME rows, in the shape a C lookup
                    // can consume. Not a second table: one loop below writes
                    // both, from `subs`, so they cannot say different things.
                    //
                    // Why a second SPELLING is unavoidable. C++ reads the form
                    // above by defining `NROS_DECLARED_QOS_ROW` and evaluating
                    // a `constexpr` search over the array it builds. C has no
                    // `constexpr`, so a C lookup has to be built by the
                    // PREPROCESSOR -- and a macro parameter of the caller is
                    // not substituted inside a separately-defined row macro:
                    // `#define ROW(t, tp, d) ... q_type ...` sees `q_type` as
                    // an ordinary identifier, never as `LOOKUP`'s argument.
                    // Measured on gcc 15 and clang 20: `use of undeclared
                    // identifier 'q_type'`. The query therefore has to travel
                    // THROUGH the list, which means the list takes it.
                    s.push_str(
                        "\n/* X-macro, QUERY form (phase-454 W10) -- the same rows, with the\n \
                         * row macro AND the queried (type, topic) passed in, so\n \
                         * `nros/declared_qos.h` can expand the table into ONE constant\n \
                         * expression a C11 `_Static_assert` accepts. C++ reads the form\n \
                         * above; C reads this one. */\n",
                    );
                    s.push_str(
                        "#define NROS_DECLARED_QOS_ROWS_Q(NROS_DECLARED_QOS_ROW_Q, \\\n        \
                         nros_q_type, nros_q_topic) \\\n",
                    );
                    for r in &subs {
                        let dds = dds_type_name(&r.type_name);
                        s.push_str(&format!(
                            "    NROS_DECLARED_QOS_ROW_Q(\"{}\", \"{}\", {}, nros_q_type, \
                             nros_q_topic) \\\n",
                            c_escape(&dds),
                            c_escape(&r.topic),
                            r.depth
                        ));
                        if dds != r.type_name {
                            s.push_str(&format!(
                                "    NROS_DECLARED_QOS_ROW_Q(\"{}\", \"{}\", {}, nros_q_type, \
                                 nros_q_topic) \\\n",
                                c_escape(&r.type_name),
                                c_escape(&r.topic),
                                r.depth
                            ));
                        }
                    }
                    s.push_str("    /* end */\n");
                    let n_rows: usize = subs
                        .iter()
                        .map(|r| {
                            if dds_type_name(&r.type_name) == r.type_name {
                                1
                            } else {
                                2
                            }
                        })
                        .sum();
                    s.push_str(&format!("#define NROS_DECLARED_QOS_ROW_COUNT {n_rows}\n"));
                }
            }
        }
        s.push_str("\n#endif /* NROS_DECLARED_QOS_GENERATED_H */\n");
        s
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
                            // phase-403 step 2 -- present ONLY when declared.
                            // A `"depth": 0` or a `"depth": null` on every row
                            // would make "nobody said" and "said 0" the same
                            // JSON, which is the collapse this whole inventory
                            // is built to avoid.
                            if let Some(d) = e.depth {
                                r.insert("depth".into(), d.into());
                            }
                            // phase-454 W8 -- present ONLY when the endpoint
                            // carries them, for the reason `depth` is: a
                            // `"buffer": null` on every row would make "nobody
                            // said" and "said latest" the same JSON, and
                            // `latest` is the schema's DEFAULT, so that
                            // collapse would read as a statement.
                            if let Some(b) = e.buffer {
                                r.insert(
                                    "buffer".into(),
                                    crate::queue_depth::buffer_spelling(b).into(),
                                );
                            }
                            if let Some(p) = e.publish_rate {
                                r.insert("publish_rate_hz".into(), p.hz().into());
                            }
                            if let Some(d) = e.drain_rate {
                                r.insert("drain_rate_hz".into(), d.hz().into());
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
                // Issue 1270 -- additive: the session queryable demand and the
                // runtime's share of it.
                doc.insert("max_queryables".into(), k.max_queryables.into());
                doc.insert("infra_queryables".into(), k.infra_queryables.into());
                // phase-412 W2 / issue 1130 -- additive, like the two above.
                doc.insert("max_liveliness".into(), k.max_liveliness.into());
                doc.insert("max_cell_entities".into(), k.max_cell_entities.into());
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
        // phase-403 step 2 -- the declared depths, as their own view. Same
        // three states as the two above: a `status` is always present, the rows
        // only when it resolved, and `undeclared` says how much of the image
        // stayed silent, which is the number a size consumer refuses on.
        {
            let depths = self.declared_depths();
            let mut m = serde_json::Map::new();
            m.insert("status".into(), depths.tag().into());
            match &depths {
                DeclaredDepths::Refused { reason } => {
                    m.insert("reason".into(), reason.clone().into());
                }
                DeclaredDepths::Resolved {
                    rows,
                    undeclared,
                    undeclared_subscriptions,
                    undeclared_publishers,
                } => {
                    m.insert("undeclared".into(), (*undeclared).into());
                    // Per-kind, because the broad number answers no consumer's
                    // question: each term prices one kind and must refuse on
                    // its own kind's silence (issue 1227 for subscriptions,
                    // phase-454 W2 for publishers).
                    m.insert(
                        "undeclared_subscriptions".into(),
                        (*undeclared_subscriptions).into(),
                    );
                    m.insert(
                        "undeclared_publishers".into(),
                        (*undeclared_publishers).into(),
                    );
                    m.insert(
                        "endpoints".into(),
                        rows.iter()
                            .map(|r| {
                                let mut o = serde_json::Map::new();
                                o.insert("kind".into(), r.kind.tag().into());
                                o.insert("type_name".into(), r.type_name.clone().into());
                                o.insert(
                                    "dds_type_name".into(),
                                    dds_type_name(&r.type_name).into(),
                                );
                                o.insert("name".into(), r.topic.clone().into());
                                o.insert("depth".into(), r.depth.into());
                                serde_json::Value::Object(o)
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    );
                }
            }
            doc.insert("declared_depths".into(), serde_json::Value::Object(m));
        }
        // phase-454 W3 (issue 1256) -- the OTHER THREE policies, as their own
        // view with its own status. Its own, and not three more columns on the
        // block above, because the two statuses genuinely differ: `history:
        // keep_all` REFUSES the depth table and leaves this one resolved
        // (RFC-0100 D6 -- refusal is per fact), and a reader that found the
        // policies inside a refused `declared_depths` would lose the only
        // statement that explains the refusal.
        {
            let qos = self.declared_qos();
            let mut m = serde_json::Map::new();
            m.insert("status".into(), qos.tag().into());
            match &qos {
                DeclaredQos::Refused { reason } => {
                    m.insert("reason".into(), reason.clone().into());
                }
                DeclaredQos::Resolved { rows, .. } => {
                    // Per policy AND per kind. One "undeclared" number over
                    // three independent questions would answer none of them:
                    // an image can state `reliability` on every subscription
                    // and `durability` on none, which lets XRCE size its
                    // reliable buffers and must still refuse a transient-local
                    // retention budget.
                    for policy in ALL_QOS_POLICY_KINDS {
                        for (kind, suffix) in [
                            (EntityKind::Subscription, "subscriptions"),
                            (EntityKind::Publisher, "publishers"),
                        ] {
                            if let Some(n) = qos.undeclared(*policy, kind) {
                                m.insert(format!("undeclared_{}_{suffix}", policy.tag()), n.into());
                            }
                        }
                    }
                    m.insert(
                        "endpoints".into(),
                        rows.iter()
                            .map(|r| {
                                let mut o = serde_json::Map::new();
                                o.insert("kind".into(), r.kind.tag().into());
                                o.insert("type_name".into(), r.type_name.clone().into());
                                o.insert(
                                    "dds_type_name".into(),
                                    dds_type_name(&r.type_name).into(),
                                );
                                o.insert("name".into(), r.topic.clone().into());
                                // A policy this endpoint did not state is
                                // ABSENT from its object, never `null` and
                                // never a default: "nobody said" is the claim,
                                // and a defaulted `"volatile"` here would be
                                // the silent drop this wave exists to end.
                                for policy in ALL_QOS_POLICY_KINDS {
                                    if let Some(v) = r.spelling(*policy) {
                                        o.insert(policy.tag().into(), v.into());
                                    }
                                }
                                serde_json::Value::Object(o)
                            })
                            .collect::<Vec<_>>()
                            .into(),
                    );
                }
            }
            doc.insert("declared_qos".into(), serde_json::Value::Object(m));
        }
        // phase-446 W4 -- the parameter store, same three-state shape: a
        // `status` always, the numbers only when every node declared.
        {
            let mut m = serde_json::Map::new();
            m.insert("status".into(), self.params.tag().into());
            if let ParamDeclarations::Refused { reason } = &self.params {
                m.insert("reason".into(), reason.clone().into());
            }
            if let Some(z) = self.params.sizing() {
                m.insert("declared".into(), z.declared.into());
                m.insert("max_parameters".into(), z.max_parameters.into());
                m.insert("max_param_name_len".into(), z.max_param_name_len.into());
                for (knob, cap) in z.capacities() {
                    let v: serde_json::Value = match cap {
                        ParamCapacity::Unused => 0.into(),
                        ParamCapacity::NeededBy(p) => {
                            format!("board must state it (needed by {})", p.token()).into()
                        }
                    };
                    m.insert(knob.to_ascii_lowercase(), v);
                }
            }
            // phase-446 F3 -- the parameter services' half of the declaration.
            if let Some(shapes) = self.params.service_shapes() {
                m.insert(
                    "service_shape".into(),
                    ParamServiceShape::token(&shapes).into(),
                );
            }
            doc.insert("params".into(), serde_json::Value::Object(m));
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
        // WHERE THE COMPOSITION SOURCE WENT, AND WHY IT IS NOT HERE (issue 1228).
        //
        // These BYTES are hashed. `nros_reconfigure_snapshot` compares this
        // file's content to decide whether the mid-configure producer's answer
        // differs from the one this pass's readers already consumed, and a
        // difference costs a whole extra configure pass.
        //
        // Two renderings here depended on WHICH COMPOSER ran rather than on the
        // image, and MEASURED on `demo_bringup:zephyr` (native_sim/native/64)
        // they were the ENTIRE difference between `nros build`'s stage 3.5 seed
        // and the producer that follows it -- every number identical:
        //
        //   * `self.source`. Stage 3.5 reads the model alone; the producer
        //     reads `nros-metadata.json` merged with the model, so it names
        //     both.
        //   * the per-component provenance line's PACKAGE. A model row's `pkg`
        //     is the node FQN (`/talker`) because the model names nodes, not
        //     ament packages; a merged row keeps the declaration's (`talker_pkg`).
        //
        // Both moved to the two artifacts a byte comparison cannot reach:
        // `nros/entity_inventory.json`'s `"source"` and `"components"` keys,
        // rendered from this same data model by the same call, and
        // `resolved.toml`'s `[provenance]`. The block below is a CONSTANT --
        // identical from either composer -- because a pointer naming the file
        // it came from would be this defect again.
        s.push_str(
            "# The composing SOURCE is deliberately NOT a variable here: this fragment's\n\
             # bytes are hashed to decide whether a re-configure is needed, and the source\n\
             # is composer-dependent rather than image-dependent (issue 1228). Read it from\n\
             # `nros/entity_inventory.json` (\"source\"), or from `resolved.toml`'s\n\
             # [provenance] for a build that ran `nros build`'s resolve phase.\n",
        );
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
                // Issue 1228 -- component, NOT `pkg::component`, and SORTED.
                //
                // The package a row belongs to is composer-dependent (a model
                // row's is the node FQN, a merged row's the ament package) and
                // so is the order (the merge lists declaration rows first, the
                // model lists them sorted), and this file's bytes are hashed.
                // The component NAME is the join key `merged_per_kind_max` uses,
                // so it is the half that agrees whenever the counts do. Sorting
                // on the rendered line makes equal content mean equal bytes
                // with no tie-break left to the composer.
                // `entity_inventory.json`'s `components` carries the package.
                //
                // The REFUSAL reasons below still spell `pkg::component`, and
                // deliberately: they are what a human reads to find the
                // declaration to fix, and they cannot cost a configure pass.
                // A refusal reaching this fragment while the seed derived is a
                // DISAGREEMENT ON THE NUMBERS -- case C of
                // `tests/cmake-resolved-seed-tests.sh` -- so the pass is
                // already spent on the count, not on the prose. Both refusing
                // means the seed does not exist: stage 3.5 writes no
                // `resolved.cmake` for an image it could not answer for. The
                // one shape that would matter -- counts agreeing while the
                // TYPE sets refuse in both -- needs an entity with no type,
                // and since phase-412 retired `ENTITIES` every entity in the
                // merged inventory comes from the model, which always carries
                // `wiring.msg_type`.
                s.push_str(
                    "# Where the slots came from -- component = entities/slots. The PACKAGE is\n\
                     # in `nros/entity_inventory.json`; it is composer-dependent, and this\n\
                     # file's bytes decide whether cmake runs again (issue 1228).\n",
                );
                let mut rows: Vec<String> = k
                    .per_component
                    .iter()
                    .map(|(_pkg, comp, count, slots)| {
                        format!("#   {comp} = {count} entities, {slots} slots\n")
                    })
                    .collect();
                rows.sort();
                for row in rows {
                    s.push_str(&row);
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
                // Issue 0900 -- of those slots, how many the arena must budget
                // at the ACTION entry size (18,048 B at the defaults) rather
                // than the pub/sub one (3,584 B). A talker derives 0 here and
                // stops carrying 74,240 bytes of task stack for an entity it
                // never constructs.
                s.push_str(
                    "# Action clients AND action servers: the knob is named for\n                     # clients because build.rs picked one as the worst case, but\n                     # the arena stores ActionServerArenaEntry too (issue 0900).\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_EXECUTOR_ACTION_CLIENTS {})\n",
                    k.heavy_slots
                ));
                // phase-412 W1 -- the SESSION pools. Separate from the slot
                // demand above: a publisher claims no callback slot but does
                // claim a session slot, and a declared action claims several
                // of these for the one entity it declares.
                s.push_str(
                    "# Session pools. A declared action is ONE entity that costs\n                     # SEVERAL session slots: a server opens 3 queryables and 2\n                     # publishers, a client 1 subscription. The multipliers live\n                     # beside the calls that decide them and are held there by\n                     # check-infra-queryable-counts.\n                     # INCLUDED since issue 1270: the parameter family (6 per node)\n                     # and the lifecycle family (5), when the bringup declares\n                     # them -- attributed on the line below the knob. These are\n                     # DEFAULTS and not ceilings: a stated knob wins.\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_MAX_SUBSCRIBERS {})\n",
                    k.max_subscribers
                ));
                s.push_str(&format!(
                    "set(NROS_DERIVED_RMW_SUBSCRIBER_SLOTS {})\n",
                    k.max_subscribers
                ));
                s.push_str(&format!(
                    "set(NROS_DERIVED_MAX_PUBLISHERS {})\n",
                    k.max_publishers
                ));
                s.push_str(&format!(
                    "set(NROS_DERIVED_MAX_QUERYABLES {})\n",
                    k.max_queryables
                ));
                // Issue 1270 -- attribute the runtime's share, so a reader can
                // tell the application's servers from the ones created FOR it.
                let param_share = PARAM_SERVICE_QUERYABLES * k.param_service_nodes;
                s.push_str(&format!(
                    "#   of which {} are the runtime's own servers: {} for param_services on \
                     {} node(s), {} for lifecycle.\n",
                    k.infra_queryables,
                    param_share,
                    k.param_service_nodes,
                    k.infra_queryables - param_share
                ));
                s.push_str(
                    "# One node per declared component. Over-counts if two share\n                     # a name (slots are keyed by name); UNDER-counts only for a\n                     # bridge, whose two nodes are runtime strings declared\n                     # nowhere -- that path names this knob when the table fills.\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_EXECUTOR_MAX_NODES {})\n",
                    k.max_nodes
                ));
                // phase-412 W2 -- the liveliness pool. Local tokens only.
                s.push_str(
                    "# Liveliness tokens THIS session declares (not the peer graph): one\n                     # for the session's own node, one per node name (plus the executor's\n                     # when lifecycle is declared), and one per publisher, subscriber,\n                     # service server and service client -- the session pools above plus\n                     # three clients per action client. Exhaustion names the knob.\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_MAX_LIVELINESS {})\n",
                    k.max_liveliness
                ));
                // Issue 1130 -- the knob-capped cell registries.
                s.push_str(
                    "# Per-kind cell registry capacity for a class that states no\n                     # ENTITY_BOUNDS: the largest single kind in any one component, over\n                     # publishers, service servers/clients, action servers/clients.\n                     # Zero is a legal answer; an explicit ENTITY_BOUNDS still wins.\n",
                );
                s.push_str(&format!(
                    "set(NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES {})\n",
                    k.max_cell_entities
                ));
                // Issue 1198 -- the SCHEDULING half. Slot 0 is reserved for the
                // default Fifo context, so the demand is 1 + what the schedule
                // creates; the Rust runtime creates none (it mutates slot 0),
                // the C/C++ entry pack creates one per tier.
                s.push_str(
                    "# Scheduling-context slots: slot 0 is the reserved default Fifo\n                     # context, plus one per tier the schedule can create. A bringup\n                     # that authors no tiers can still resolve one per node\n                     # (derive_tiers_from_contracts), so the second term is the LARGER\n                     # of the authored tier count and the node count.\n",
                );
                s.push_str(&format!("set(NROS_DERIVED_EXECUTOR_MAX_SC {})\n", k.max_sc));
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
        // phase-403 step 2 -- the QoS DEPTHS. The arena's per-subscription cost
        // is `(depth + 1) * bound + (depth + 1) * 8`, so depth is a MULTIPLIER
        // on the type bound the two views above supply, and no arena can derive
        // without it. Emitted in both branches for the same reason they are: an
        // absent variable would read as "every endpoint is depth 0".
        s.push_str(&render_declared_depths(&self.declared_depths()));
        // phase-454 W3 -- the other three QoS policies. Rendered in both
        // branches for the same reason every view above is: an absent list
        // would read as "no endpoint stated a reliability", which is the claim
        // a refusal must never make on an image's behalf.
        s.push_str(&render_declared_qos(&self.declared_qos()));
        // phase-446 W4 -- the PARAMETER STORE. Independent of the entity
        // derivation above, so it renders in either branch.
        s.push_str(&render_param_store(&self.params));
        s
    }

    /// The environment projection -- the carrier that reaches a cargo build.
    ///
    /// One `KEY=VALUE` per line, and NOTHING when the derivation refused: an
    /// absent variable leaves `nros-node/build.rs` on its own default, which is
    /// rung 4 of the precedence ladder and the correct outcome for "no answer".
    pub fn to_env(&self) -> String {
        match self.derive() {
            // Issue 0900 -- both, or neither. `NROS_EXECUTOR_ACTION_CLIENTS`
            // is only meaningful against the `MAX_CBS` it is clamped to, and
            // emitting one without the other would size an arena against a
            // slot count from a different rung.
            //
            // Issue 1130 -- the cell bound travels too. It is independent of
            // the pair above and a cargo leaf reads it (`nros/build.rs`).
            // `ZPICO_MAX_LIVELINESS` does NOT: like the queryable count it
            // includes the param/lifecycle servers only when this inventory saw
            // the model, and a bare env carrier cannot say whether it did.
            Derivation::Derived(k) => format!(
                "NROS_EXECUTOR_MAX_CBS={}\nNROS_EXECUTOR_ACTION_CLIENTS={}\n\
                 NROS_RUNTIME_MAX_CELL_ENTITIES={}\n",
                k.max_cbs, k.heavy_slots, k.max_cell_entities
            ),
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

/// The declared-depth view, as CMake (phase-403 step 2).
///
/// Publishes these, and the counts are the point:
///
///   `NROS_ENTITY_DECLARED_DEPTHS`                   SUBSCRIPTION `type|topic=depth`
///                                                   triples, `;`-joined
///   `NROS_ENTITY_DECLARED_DEPTH_COUNT`              how many subscriptions stated one
///   `NROS_ENTITY_UNDECLARED_DEPTH_COUNT`            how many endpoints of ANY
///                                                   depth-carrying kind could have
///                                                   and did not
///   `NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION`  the same, subscriptions only
///   `NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER`         PUBLISHER triples (phase-454 W2)
///   `NROS_ENTITY_DECLARED_DEPTH_COUNT_PUBLISHER`    how many publishers stated one
///   `NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER`  the same, publishers only
///   `NROS_ENTITY_DECLARED_DEPTH_STATUS`             resolved | refused
///
/// A consumer that sizes from depth must read the UNDECLARED count for ITS OWN
/// KIND and refuse when it is non-zero. Reading only the list would size an
/// image from the subset of its endpoints that happened to be annotated, which
/// is the exact under-report `ENTITIES NONE` exists to prevent one level up.
///
/// # Why the publisher list is a SEPARATE VARIABLE (phase-454 W2)
///
/// `NROS_ENTITY_DECLARED_DEPTHS` is not a general depth table; it is the input
/// to ONE term, `nros-node/build.rs::subs_arena`, which refuses to size unless
/// `declared.len() == subs` -- the number of SUBSCRIPTIONS. That equality is
/// how the lane distinguishes "every subscription declared" from "some did",
/// and it is the whole guard: when it fails, every declaring image silently
/// falls back to `subs * pubsub_entry_at_default`, an arena measured at
/// 207,096 bytes against 71,664 on the reference island. Appending publisher
/// triples to the same list would break it for every image that declares both
/// -- not loudly, but as a large silent regression in exactly the images that
/// took the trouble to declare.
///
/// So the split is structural rather than cosmetic, and the legacy name keeps
/// the scope its one consumer already assumed. Two other consumers assumed it
/// too and narrowed BY HAND: `to_declared_qos_header` filters
/// `kind == Subscription` before emitting a row (a publisher and a subscription
/// on one topic would be two rows with one key), and `_nros_qos_depth_env` in
/// `cmake/NanoRosEntityFacts.cmake` takes a MAX over the list to bill a
/// subscription's receive region. A depth for a kind none of them price
/// belongs in a variable none of them read.
fn render_declared_depths(d: &DeclaredDepths) -> String {
    let mut s = String::from(
        "# phase-403 step 2 -- the DECLARED QoS history depths. Depth is a MULTIPLIER on\n\
         # the type bound above: the arena's per-subscription cost is\n\
         # `(depth + 1) * bound + (depth + 1) * 8`, measured at 86108 bytes for ten\n\
         # subscriptions at the ROS default depth 10 and 24516 at depth 1.\n\
         # A consumer that sizes from these must refuse while\n\
         # NROS_ENTITY_UNDECLARED_DEPTH_COUNT is non-zero: an endpoint that stated no\n\
         # depth has not opted in, and BOTH defaults are wrong -- 10 inflates an image\n\
         # that meant 1 tenfold, and 1 under-sizes one that took the default.\n",
    );
    s.push_str(&format!(
        "set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"{}\")\n",
        d.tag()
    ));
    match d {
        DeclaredDepths::Refused { reason } => {
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_DEPTH_REASON \"{}\")\n",
                cmake_escape(reason)
            ));
            s.push_str(
                "# No depth table. A consumer that needs one must REFUSE -- a partial table\n\
                 # is indistinguishable from an image whose endpoints all took the default.\n",
            );
        }
        DeclaredDepths::Resolved {
            rows,
            undeclared,
            undeclared_subscriptions,
            undeclared_publishers,
        } => {
            let triples = |kind: EntityKind| -> Vec<String> {
                rows.iter()
                    .filter(|r| r.kind == kind)
                    .map(|r| format!("{}|{}={}", r.type_name, r.topic, r.depth))
                    .collect()
            };
            let subs = triples(EntityKind::Subscription);
            // phase-454 W8 -- which of those rows were DERIVED rather than
            // stated. The main list keeps carrying both, because its one
            // consumer (`subs_arena`) wants the size and a default is exactly
            // what a default is for. This is the provenance beside it, so a
            // reader that needs to distinguish them can, and nobody has to
            // recover it by diffing against the contract.
            let derived: Vec<String> = rows
                .iter()
                .filter(|r| r.source == DepthSource::DerivedFromRates)
                .map(|r| format!("{}|{}={}", r.type_name, r.topic, r.depth))
                .collect();
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_DEPTHS \"{}\")\n",
                subs.join(";")
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_DEPTH_COUNT {})\n",
                subs.len()
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT {undeclared})\n"
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION {undeclared_subscriptions})\n"
            ));
            // phase-454 W2 -- the publisher half, in its OWN names. See the
            // function's doc comment for why appending to the list above would
            // be a silent arena regression rather than an extension.
            let publishers = triples(EntityKind::Publisher);
            s.push_str(
                "# phase-454 W2 -- the PUBLISHER depths. A separate list because the one\n\
                 # above is the input to a SUBSCRIPTION term whose guard is\n\
                 # `declared.len() == <subscription count>`. Nothing prices these yet;\n\
                 # publisher-side retention for `transient_local` durability is what will.\n",
            );
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER \"{}\")\n",
                publishers.join(";")
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_DEPTH_COUNT_PUBLISHER {})\n",
                publishers.len()
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER {undeclared_publishers})\n"
            ));
            // phase-454 W8 (RFC-0100 D9) -- the DERIVED subset, always emitted.
            //
            // Emitted even when empty, which is every image today: an empty
            // list is the published fact "this image derived none", and leaving
            // the variable out would make it indistinguishable from an older
            // CLI that could not derive any. That distinction is the whole
            // reason the schema version below bumps.
            s.push_str(
                "# phase-454 W8 -- which rows above were DERIVED from a `buffer: queue`\n\
                 # endpoint's publish and drain rates rather than STATED by the contract\n\
                 # (RFC-0100 D9). They are in the list above because a default is what the\n\
                 # arena wants; they are NOT in the declared-QoS header, because a default\n\
                 # must never become a `static_assert` a call site has to match.\n",
            );
            s.push_str(&format!(
                "set(NROS_ENTITY_DERIVED_DEPTHS \"{}\")\n",
                derived.join(";")
            ));
            s.push_str(&format!(
                "set(NROS_ENTITY_DERIVED_DEPTH_COUNT {})\n",
                derived.len()
            ));
        }
    }
    s
}

/// The other three QoS policies, as CMake -- phase-454 W3, issue 1256.
///
/// Publishes, per policy P in {RELIABILITY, DURABILITY, HISTORY}:
///
///   `NROS_ENTITY_DECLARED_QOS_STATUS`            resolved | refused
///   `NROS_ENTITY_DECLARED_QOS_REASON`            prose, when refused
///   `NROS_ENTITY_DECLARED_<P>`                   SUBSCRIPTION `type|topic=value`
///   `NROS_ENTITY_DECLARED_<P>_PUBLISHER`         the publisher list
///   `NROS_ENTITY_UNDECLARED_<P>_COUNT_SUBSCRIPTION`
///   `NROS_ENTITY_UNDECLARED_<P>_COUNT_PUBLISHER`
///
/// # Why one list per policy, and per kind
///
/// The grammar is `type|topic=value`, the SAME one the depth lists use, so a
/// consumer already has the parse. Packing three policies into one row would
/// need a second separator inside a field whose separator is already `|`, and
/// a cmake list whose elements contain the list separator is the class of bug
/// that reads as working until one topic is unusual.
///
/// Per KIND for the reason phase-454 W2 split the depth lists and issue 1227
/// split the counts before it: a consumer prices ONE kind, and a count over
/// both means one unannotated endpoint of the other kind pins it on its worst
/// case forever. The first consumer here will be XRCE's, whose two 64 KiB
/// `*_reliable_buf` are a per-SESSION cost gated on whether anything in the
/// image asked for `reliable` -- and that is a question about this image's
/// endpoints, not about a default.
///
/// Nothing reads these yet. They are wave W6's inputs, and W3's job is that the
/// fact stated in the contract reaches the build at all; see the module header
/// on why a declaration that is legal to write and silently dropped is the
/// worst of the three possible outcomes.
fn render_declared_qos(q: &DeclaredQos) -> String {
    let mut s = String::from(
        "# phase-454 W3 (issue 1256) -- the DECLARED QoS policies other than the depth.\n\
         # A contract has been able to state `reliability`, `durability` and `history`\n\
         # per endpoint since the schema had a `qos:` key; nano-ros read `depth` and\n\
         # dropped the rest, so `reliability: best_effort` was legal to write, legal to\n\
         # resolve, and read by nobody.\n\
         #\n\
         # ABSENCE IS NOT A VALUE. An endpoint missing from a list did not state that\n\
         # policy; it did not state `volatile`. A consumer that sizes from one must\n\
         # read the UNDECLARED count for ITS OWN policy and ITS OWN kind, and refuse on\n\
         # the safe side of its own question -- for XRCE's two 64 KiB reliable buffers\n\
         # the safe side is to assume RELIABLE and pay them (RFC-0100 D6).\n\
         #\n\
         # `history: keep_all` is NOT refused here. It refuses the DEPTH table above,\n\
         # because a KEEP_ALL queue has no static bound and a depth stated beside it\n\
         # prices nothing. The statement itself is well declared and a consumer that\n\
         # reads history (an XRCE STREAM_HISTORY, a Cyclone resource limit) must see\n\
         # it -- refusal is per fact, never global.\n",
    );
    s.push_str(&format!(
        "set(NROS_ENTITY_DECLARED_QOS_STATUS \"{}\")\n",
        q.tag()
    ));
    match q {
        DeclaredQos::Refused { reason } => {
            s.push_str(&format!(
                "set(NROS_ENTITY_DECLARED_QOS_REASON \"{}\")\n",
                cmake_escape(reason)
            ));
            s.push_str(
                "# No policy table. A consumer that needs one must REFUSE -- a partial table\n\
                 # is indistinguishable from an image whose endpoints all took the default.\n",
            );
        }
        DeclaredQos::Resolved { rows, .. } => {
            for policy in ALL_QOS_POLICY_KINDS {
                let infix = policy.cmake_infix();
                for (kind, suffix) in [
                    (EntityKind::Subscription, ""),
                    (EntityKind::Publisher, "_PUBLISHER"),
                ] {
                    let triples: Vec<String> = rows
                        .iter()
                        .filter(|r| r.kind == kind)
                        .filter_map(|r| {
                            r.spelling(*policy)
                                .map(|v| format!("{}|{}={v}", r.type_name, r.topic))
                        })
                        .collect();
                    s.push_str(&format!(
                        "set(NROS_ENTITY_DECLARED_{infix}{suffix} \"{}\")\n",
                        triples.join(";")
                    ));
                }
                for (kind, suffix) in [
                    (EntityKind::Subscription, "SUBSCRIPTION"),
                    (EntityKind::Publisher, "PUBLISHER"),
                ] {
                    // `undeclared` returns `None` only on a refusal, and this
                    // arm is the resolved one -- so the `unwrap_or` never fires
                    // and a 0 it wrote would be a lie. Spelled as a match so a
                    // future kind cannot silently become "0 undeclared".
                    let n = q
                        .undeclared(*policy, kind)
                        .expect("a resolved table counts both topic kinds");
                    s.push_str(&format!(
                        "set(NROS_ENTITY_UNDECLARED_{infix}_COUNT_{suffix} {n})\n"
                    ));
                }
            }
        }
    }
    s
}

/// A C string literal's body. The table's keys are ROS type and topic names, so
/// in practice nothing here needs escaping -- which is exactly why it is done
/// anyway: an unescaped quote or backslash reaching a generated header is a
/// compile error in a file nobody edits, and the fix would be invisible.
/// phase-446 W4 -- the parameter store, as CMake.
///
/// `NROS_PARAM_DECLARATION_STATUS` is always set. A number is set only when
/// every node declared, and a capacity some declared type needs gets NO
/// number at all: it gets `NROS_PARAM_NEEDS_<knob>` naming the parameter, and
/// the board supplies the number or `nros-params`' build script refuses. That
/// build script is the one place every rung (environment, Kconfig, the
/// `[knobs.params]` board rung) meets, so it is the one place the refusal can
/// be right on every lane.
fn render_param_store(p: &ParamDeclarations) -> String {
    let mut s = String::from(
        "# The PARAMETER STORE (phase-446 W4), sized from the contract's `params:`.\n\
         # Every slot is as large as the largest value the limits allow, so a\n\
         # capacity no declared type uses is 0, and a capacity one does use comes\n\
         # from the BOARD -- a contract states names and types, never sizes.\n\
         # `absent` is not zero: an image whose contract declares no parameters\n\
         # keeps every store knob on its configured value.\n",
    );
    s.push_str(&format!(
        "set(NROS_PARAM_DECLARATION_STATUS \"{}\")\n",
        p.tag()
    ));
    if let ParamDeclarations::Refused { reason } = p {
        s.push_str(&format!(
            "set(NROS_PARAM_DECLARATION_REASON \"{}\")\n",
            cmake_escape(reason)
        ));
    }
    let Some(z) = p.sizing() else {
        return s;
    };
    s.push_str(&format!("set(NROS_PARAM_DECLARED_COUNT {})\n", z.declared));
    s.push_str(&format!(
        "# {} declared + one `{SEEDED_PARAMETER}` per node (seeded on every node).\n",
        z.declared
    ));
    s.push_str(&format!(
        "set(NROS_DERIVED_MAX_PARAMETERS {})\n",
        z.max_parameters
    ));
    s.push_str(&format!(
        "set(NROS_DERIVED_MAX_PARAM_NAME_LEN {})\n",
        z.max_param_name_len
    ));
    for (knob, cap) in z.capacities() {
        match cap {
            ParamCapacity::Unused => {
                s.push_str(&format!("set(NROS_DERIVED_{knob} 0)\n"));
            }
            ParamCapacity::NeededBy(d) => {
                s.push_str(&format!(
                    "# NROS_{knob}: `{}` on {} is declared `{}`, so the board states it.\n",
                    d.name,
                    d.node,
                    d.ty.as_str()
                ));
                s.push_str(&format!(
                    "set(NROS_PARAM_NEEDS_{knob} \"{}\")\n",
                    cmake_escape(&d.token())
                ));
            }
        }
    }
    // phase-446 F3 -- the parameter SERVICES. Not a buffer size: the size
    // also needs the capacities above, and those are the board's, so what
    // crosses is the part the contract decides. nros-node finishes it.
    if let Some(shapes) = p.service_shapes() {
        s.push_str(
            "# The PARAMETER SERVICES (phase-446 F3): per node, what the declared\n\
             # names and types put on the wire before any capacity, as\n\
             # params:name_bytes:prefixes:prefix_bytes:strings:byte_arrays:\n\
             # bool_arrays:word_arrays:string_arrays. nros-node bounds its\n\
             # service buffer from these and the store's RESOLVED capacities.\n",
        );
        s.push_str(&format!(
            "set(NROS_PARAM_SERVICE_SHAPE \"{}\")\n",
            ParamServiceShape::token(&shapes)
        ));
    }
    s
}

fn c_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
                // issue 1033 -- the remedy must be one the caller can still
                // perform. This asserted `ENTITIES NONE` for a phase after
                // phase-412 made `ENTITIES` a FATAL_ERROR, so the test held the
                // message to advice that could not be followed.
                assert!(
                    reason.contains("contract.yaml"),
                    "names a remedy that still exists: {reason}"
                );
                assert!(
                    !reason.contains("ENTITIES"),
                    "never names a retired keyword as the remedy: {reason}"
                );
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // And no transport may carry a number.
        assert!(!inv.to_cmake().contains("NROS_DERIVED_EXECUTOR_MAX_CBS"));
        assert_eq!(inv.to_env(), "");
    }

    /// Issue 1198 — the executor's two FIXED TABLES follow the declaration.
    ///
    /// Deltas rather than absolutes where it matters: the point is that the
    /// numbers MOVE with what the image says, because the defect was that they
    /// did not — 4 node slots and 8 scheduling-context slots in every image,
    /// identical to the byte across leaves whose declarations differ.
    #[test]
    fn the_fixed_tables_follow_the_declaration() {
        let mut one = EntityInventory::new("test");
        one.insert(stated("a", "talker", &["publisher", "timer"]));
        let k = one.derive();
        let k = k.knobs().expect("derived");
        assert_eq!(k.max_nodes, 1, "one component is one node slot");
        // Slot 0 is RESERVED for the default Fifo context, so a single-node
        // image with no authored tiers still needs two.
        assert_eq!(k.max_sc, 2, "the reserved slot 0 plus this image's one");

        let mut three = EntityInventory::new("test");
        three.insert(stated("a", "one", &["timer"]));
        three.insert(stated("b", "two", &["timer"]));
        three.insert(stated("c", "three", &["timer"]));
        let k3 = three.derive();
        let k3 = k3.knobs().expect("derived");
        assert_eq!(k3.max_nodes, 3);
        assert_eq!(k3.max_sc, 4);

        // An AUTHORED tier table outranks the node count when it is larger:
        // the C/C++ entry pack creates one scheduling context per tier, and a
        // two-node image on five tiers needs five.
        let mut tiered = EntityInventory::new("test");
        tiered.insert(stated("a", "one", &["timer"]));
        tiered.insert(stated("b", "two", &["timer"]));
        tiered.set_tiers(5);
        assert_eq!(
            tiered.derive().knobs().expect("derived").max_sc,
            6,
            "five tiers is five contexts, plus the reserved slot 0"
        );
        // ... and never LOWERS it: a bringup that authors no tiers can still
        // resolve one per node (`derive_tiers_from_contracts`).
        assert_eq!(
            tiered.derive().knobs().expect("derived").max_nodes,
            2,
            "tiers do not change the node table"
        );
    }

    /// The other half of issue 1198: the numbers reach the CARRIER a build
    /// reads, not just the struct. `MAX_NODES` was correct on this road's
    /// producer for two phases while nothing carried it, and `MAX_SC` was not
    /// published at all.
    #[test]
    fn the_fixed_tables_reach_both_cargo_roads() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "talker", &["publisher", "timer"]));
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_DERIVED_EXECUTOR_MAX_NODES 1)"), "{c}");
        assert!(c.contains("set(NROS_DERIVED_EXECUTOR_MAX_SC 2)"), "{c}");

        let k = inv.derive();
        let k = k.knobs().expect("derived");
        let sidecar = crate::leaf_entity_env::render_env_sidecar(
            k,
            &crate::leaf_payload_classes::PayloadClasses::Refused {
                reason: "test".into(),
            },
            &crate::leaf_take_buffer::TakeBuffer::Refused {
                reason: "test".into(),
            },
            "test",
        );
        assert!(
            sidecar.contains("NROS_EXECUTOR_MAX_NODES = \"1\""),
            "{sidecar}"
        );
        assert!(
            sidecar.contains("NROS_EXECUTOR_MAX_SC = \"2\""),
            "{sidecar}"
        );
    }

    /// Issue 0900 — the heavy-slot count is what stops a talker carrying an
    /// arena sized for an entity it does not have.
    ///
    /// Asserted through `to_env`, not just the struct field: the env projection
    /// is the only thing a build ever reads, and the derivation was correct for
    /// two phases while nothing lowered it.
    #[test]
    fn only_action_entities_claim_a_heavy_arena_slot() {
        // A talker: publisher (no slot at all) + timer. Nothing heavy.
        let mut talker = EntityInventory::new("test");
        talker.insert(stated("a", "talker", &["publisher", "timer"]));
        assert_eq!(
            talker.to_env(),
            "NROS_EXECUTOR_MAX_CBS=1\nNROS_EXECUTOR_ACTION_CLIENTS=0\n\
             NROS_RUNTIME_MAX_CELL_ENTITIES=1\n",
            "a pub/sub-only image must budget no slot at the action size"
        );

        // An action CLIENT is heavy.
        let mut client = EntityInventory::new("test");
        client.insert(stated("a", "client", &["timer", "action_client"]));
        assert_eq!(client.derive().knobs().expect("derived").heavy_slots, 1);

        // So is an action SERVER, though the knob is named for clients: the
        // arena stores `ActionServerArenaEntry`, so advising a server image to
        // zero the knob would trade the saving for `BufferTooSmall`.
        let mut server = EntityInventory::new("test");
        server.insert(stated("a", "server", &["action_server"]));
        assert_eq!(
            server.derive().knobs().expect("derived").heavy_slots,
            1,
            "an action server occupies a heavy slot too"
        );

        // A service client/server is NOT heavy — the nearest miss, and the one
        // a name-based rule would get wrong.
        let mut svc = EntityInventory::new("test");
        svc.insert(stated("a", "svc", &["service_client", "service_server"]));
        assert_eq!(svc.derive().knobs().expect("derived").heavy_slots, 0);
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

    // -----------------------------------------------------------------
    // phase-403 step 2 -- the QoS DEPTH.
    // -----------------------------------------------------------------

    /// The grammar, in the spelling the design fixed: a NAMED attribute after
    /// the positional fields, so reliability/history/durability can follow
    /// without another grammar change, and so a fourth positional can never be
    /// ambiguous against a topic.
    #[test]
    fn a_depth_attaches_as_a_named_attribute() {
        let d =
            EntityDecl::parse("sub:nav_msgs/msg/Odometry:/localization/kinematic_state@depth=10")
                .unwrap()
                .remove(0);
        assert_eq!(d.kind, EntityKind::Subscription);
        assert_eq!(d.type_name.as_deref(), Some("nav_msgs/msg/Odometry"));
        assert_eq!(
            d.name.as_deref(),
            Some("/localization/kinematic_state"),
            "the attribute must not be left on the end of the topic"
        );
        assert_eq!(d.depth, Some(10));
    }

    /// ABSENCE IS NOT ZERO -- the rule the whole inventory turns on, restated
    /// one level down. `None` means nobody said; a size consumer refuses on it.
    #[test]
    fn an_undeclared_depth_is_none_and_never_zero() {
        let d = EntityDecl::parse("sub:std_msgs/msg/Int32:/t")
            .unwrap()
            .remove(0);
        assert_eq!(d.depth, None);
        // And the spelling that WOULD collapse them is rejected outright: a
        // KEEP_LAST(0) holds no sample, so `@depth=0` is not a smaller queue,
        // it is a typo for "I did not want to say".
        let err = EntityDecl::parse("sub:std_msgs/msg/Int32:/t@depth=0").unwrap_err();
        assert!(err.contains("states nothing"), "{err}");
        assert!(err.contains("REFUSE"), "names what absence buys: {err}");
    }

    /// An unknown attribute is an ERROR, never a skipped one -- the same rule
    /// an unknown KIND follows, and for the same reason: a silently ignored
    /// attribute is a declaration the author believes they made.
    #[test]
    fn an_unknown_attribute_is_rejected_rather_than_ignored() {
        let err = EntityDecl::parse("sub:std_msgs/msg/Int32:/t@dpeth=1").unwrap_err();
        assert!(err.contains("unknown attribute"), "{err}");
        assert!(err.contains("depth"), "names the legal set: {err}");
        // A bare attribute with no value is a typo too, not a flag.
        assert!(EntityDecl::parse("sub:std_msgs/msg/Int32:/t@depth").is_err());
        assert!(EntityDecl::parse("sub:std_msgs/msg/Int32:/t@").is_err());
        assert!(EntityDecl::parse("sub:std_msgs/msg/Int32:/t@depth=ten").is_err());
        // One entity has one depth.
        assert!(EntityDecl::parse("sub:std_msgs/msg/Int32:/t@depth=1@depth=2").is_err());
    }

    /// A timer has no QoS, so a depth on one is a statement about nothing.
    /// Rejected rather than ignored -- an author who wrote it meant something.
    #[test]
    fn a_depth_on_a_kind_with_no_qos_is_rejected() {
        let err = EntityDecl::parse("timer@depth=5").unwrap_err();
        assert!(err.contains("has no QoS"), "{err}");
        assert!(EntityDecl::parse("guard@depth=5").is_err());
        // Every other kind carries one. A publisher's depth sizes no receive
        // buffer today, and forbidding it would make the grammar say something
        // false about the QoS a publisher really has.
        for k in ALL_ENTITY_KINDS {
            assert_eq!(
                k.carries_qos_depth(),
                !matches!(k, EntityKind::Timer | EntityKind::GuardCondition),
                "{} and QoS depth",
                k.tag()
            );
        }
    }

    /// A repeat count multiplies the whole row, depth included -- otherwise
    /// `sub*3:...@depth=1` would declare one sized endpoint and two silent ones.
    #[test]
    fn a_repeat_count_carries_the_depth_to_every_copy() {
        let d = EntityDecl::parse("sub*3:std_msgs/msg/Int32:/t@depth=2").unwrap();
        assert_eq!(d.len(), 3);
        assert!(d.iter().all(|e| e.depth == Some(2)));
    }

    /// The image-wide view, and the field that makes it honest: how many
    /// endpoints COULD have declared a depth and did not. Reporting only the
    /// rows would make a partly-annotated image look fully declared.
    #[test]
    fn the_declared_depth_view_counts_what_stayed_silent() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/t@depth=1",
                "sub:nav_msgs/msg/Odometry:/k@depth=10",
                "sub:std_msgs/msg/Bool:/b",
                "pub:std_msgs/msg/Bool:/p",
                // No QoS at all, so not in the population either way.
                "timer*4",
            ],
        ));
        match inv.declared_depths() {
            DeclaredDepths::Resolved {
                rows, undeclared, ..
            } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].type_name, "nav_msgs/msg/Odometry", "sorted");
                assert_eq!(rows[0].depth, 10);
                assert_eq!(
                    undeclared, 2,
                    "the untyped-depth subscription and the publisher; the four timers \
                     carry no QoS and are not in the population"
                );
            }
            other => panic!("expected resolved, got {other:?}"),
        }
    }

    /// An incomplete image has NO depth table, for the reason it has no type
    /// set: a missing row reads as "nobody declared this endpoint", and that is
    /// the one thing the table must never say wrongly -- it is what the
    /// compile-time check treats as "assert nothing".
    #[test]
    fn an_incomplete_image_has_no_depth_table() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "one", &["sub:std_msgs/msg/Int32:/t@depth=1"]));
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        assert!(matches!(
            inv.declared_depths(),
            DeclaredDepths::Refused { .. }
        ));
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"refused\")"));
        assert!(
            !c.contains("set(NROS_ENTITY_DECLARED_DEPTHS "),
            "a refusal must publish no depth list at all: {c}"
        );
        // ...and the generated header carries no rows either, so every call
        // site in that image compiles unchecked rather than against a partial
        // table.
        let h = inv.to_declared_qos_header();
        assert!(h.contains("NROS_DECLARED_QOS_STATUS \"refused\""));
        assert!(!h.contains("#define NROS_DECLARED_QOS_ROWS"));
    }

    /// The CMake projection carries the list AND the undeclared count.
    #[test]
    fn the_cmake_projection_carries_the_depths_and_what_stayed_silent() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/t@depth=1",
                "sub:std_msgs/msg/Bool:/b",
            ],
        ));
        let c = inv.to_cmake();
        assert!(c.contains("set(NROS_ENTITY_DECLARED_DEPTHS \"std_msgs/msg/Int32|/t=1\")"));
        assert!(c.contains("set(NROS_ENTITY_DECLARED_DEPTH_COUNT 1)"));
        assert!(
            c.contains("set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT 1)"),
            "the count of endpoints that said nothing is what a size consumer \
             refuses on: {c}"
        );
        // The canonical transport carries the same three facts.
        let j = inv.to_json();
        assert!(j.contains("\"declared_depths\""));
        assert!(j.contains("\"undeclared\": 1"));
        assert!(j.contains("\"dds_type_name\": \"std_msgs::msg::dds_::Int32_\""));
        // A row with no depth carries no `depth` key at all -- `"depth": 0` and
        // `"depth": null` would each make "nobody said" look like an answer.
        assert!(!j.contains("\"depth\": 0"));
        assert!(!j.contains("\"depth\": null"));
    }

    /// The C++ table: subscriptions only, both spellings of the type, and NO
    /// `NROS_DECLARED_QOS_ROWS` at all when nothing was declared.
    #[test]
    fn the_generated_header_emits_both_type_spellings_for_subscriptions() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/chatter@depth=1",
                // A PUBLISHER with a declared depth is not in the table: it is
                // keyed `(type, topic)`, and a pub and a sub on one pair would
                // be two rows with one key. Nothing consults it yet either.
                "pub:std_msgs/msg/Int32:/chatter@depth=1",
            ],
        ));
        let h = inv.to_declared_qos_header();
        assert!(
            h.contains("NROS_DECLARED_QOS_ROW(\"std_msgs::msg::dds_::Int32_\", \"/chatter\", 1)")
        );
        assert!(h.contains("NROS_DECLARED_QOS_ROW(\"std_msgs/msg/Int32\", \"/chatter\", 1)"));
        assert!(
            h.contains("#define NROS_DECLARED_QOS_ROW_COUNT 2"),
            "one subscription, two spellings, and the publisher contributes none: {h}"
        );

        // Nothing declared: the macro stays UNDEFINED, so `declared_qos.hpp`
        // compiles an empty table. An empty ROWS list would be a different
        // claim, and C++ has no empty array anyway.
        let mut none = EntityInventory::new("test");
        none.insert(stated("a", "one", &["sub:std_msgs/msg/Int32:/t"]));
        let h = none.to_declared_qos_header();
        assert!(!h.contains("#define NROS_DECLARED_QOS_ROWS"));
        assert!(h.contains("#define NROS_DECLARED_QOS_UNDECLARED_COUNT 1"));
    }

    /// phase-454 W10 — the C list carries the SAME rows as the C++ one.
    ///
    /// Two lists exist because C has no `constexpr` and its lookup has to be
    /// built by the preprocessor, which means the queried `(type, topic)` must
    /// travel through the list to reach each row. What must never differ is
    /// WHICH rows: one loop writes both, and this is the assertion that keeps
    /// it one loop. A C table that carried fewer rows than the C++ one would
    /// leave exactly those endpoints unchecked in C while `just check cpp`
    /// stayed green over them.
    #[test]
    fn the_c_query_list_carries_the_same_rows_as_the_cpp_list() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "a",
            "one",
            &[
                "sub:std_msgs/msg/Int32:/chatter@depth=1",
                "sub:std_msgs/msg/Bool:/flag@depth=5",
            ],
        ));
        let h = inv.to_declared_qos_header();

        // The row macro AND the query are parameters, which is the whole
        // reason this list exists: `#define ROW(t, tp, d) ... q_type ...` does
        // not see a caller's `q_type`, so the query has to be passed in.
        assert!(
            h.contains(
                "#define NROS_DECLARED_QOS_ROWS_Q(NROS_DECLARED_QOS_ROW_Q, \\\n        \
                 nros_q_type, nros_q_topic)"
            ),
            "the C list must take the row macro and the queried (type, topic): {h}"
        );

        // Same four rows -- two endpoints, two type spellings each.
        for (ty, topic, depth) in [
            ("std_msgs::msg::dds_::Int32_", "/chatter", 1),
            ("std_msgs/msg/Int32", "/chatter", 1),
            ("std_msgs::msg::dds_::Bool_", "/flag", 5),
            ("std_msgs/msg/Bool", "/flag", 5),
        ] {
            assert!(
                h.contains(&format!(
                    "NROS_DECLARED_QOS_ROW(\"{ty}\", \"{topic}\", {depth})"
                )),
                "the C++ list is missing ({ty}, {topic}, {depth}): {h}"
            );
            assert!(
                h.contains(&format!(
                    "NROS_DECLARED_QOS_ROW_Q(\"{ty}\", \"{topic}\", {depth}, nros_q_type, \
                     nros_q_topic)"
                )),
                "the C list is missing ({ty}, {topic}, {depth}): {h}"
            );
        }
        assert_eq!(
            h.matches("NROS_DECLARED_QOS_ROW(\"").count(),
            h.matches("NROS_DECLARED_QOS_ROW_Q(\"").count(),
            "the two lists must have the same number of rows, or one language checks \
             endpoints the other does not: {h}"
        );

        // Nothing declared: NEITHER list is defined, so C gets an empty table
        // the same way C++ does and every call site compiles unchecked.
        let mut none = EntityInventory::new("test");
        none.insert(stated("a", "one", &["sub:std_msgs/msg/Int32:/t"]));
        let h = none.to_declared_qos_header();
        assert!(!h.contains("#define NROS_DECLARED_QOS_ROWS_Q"));
    }

    /// [`dds_type_name`] MIRRORS `packs/cpp/message.hpp.jinja`, so it is held
    /// to it. The two live in different crates and the C++ side is a Tera
    /// template, so this is the only thing standing between a mangling change
    /// and a table whose keys match nothing -- which would leave every
    /// `static_assert` in the tree vacuously true.
    #[test]
    fn the_dds_spelling_matches_the_cpp_template() {
        let tpl = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../rosidl-codegen/packs/cpp/message.hpp.jinja");
        let text =
            std::fs::read_to_string(&tpl).unwrap_or_else(|e| panic!("read {}: {e}", tpl.display()));
        assert!(
            text.contains(
                "static constexpr const char* TYPE_NAME = \
                 \"{{ package_name }}::msg::dds_::{{ message_name }}_\";"
            ),
            "the C++ codegen no longer emits the TYPE_NAME spelling `dds_type_name` \
             produces. Update BOTH, or the declared-QoS table keys on a name no \
             message class carries and every NROS_SUBSCRIBE assertion silently \
             passes.\n{}",
            tpl.display()
        );
        assert_eq!(
            dds_type_name("std_msgs/msg/Int32"),
            "std_msgs::msg::dds_::Int32_"
        );
        // Already mangled, or not a three-segment ROS name: passed through.
        // Guessing a mangling for a shape the codegen does not emit would put a
        // key in the table that nothing can ever match.
        assert_eq!(
            dds_type_name("std_msgs::msg::dds_::Int32_"),
            "std_msgs::msg::dds_::Int32_"
        );
        assert_eq!(dds_type_name("Weird"), "Weird");
        assert_eq!(dds_type_name("a/b"), "a/b");
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
        // Issue 1228 — the component, WITHOUT its package. The package a row
        // belongs to depends on which composer built the inventory (the model
        // states the node FQN, the merge the ament package), and this file's
        // bytes decide whether cmake configures again; `entity_inventory.json`
        // carries it instead.
        assert!(c.contains("#   one = 7 entities, 3 slots"), "{c}");
        assert!(!c.contains("#   a::one = "), "{c}");
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
        // Issue 0900 — both knobs travel together; none of these three kinds
        // is heavy, so the arena budgets no slot at the action size.
        assert_eq!(
            inv.to_env(),
            "NROS_EXECUTOR_MAX_CBS=3\nNROS_EXECUTOR_ACTION_CLIENTS=0\n\
             NROS_RUNTIME_MAX_CELL_ENTITIES=1\n"
        );
        inv.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        assert_eq!(inv.to_env(), "");
    }

    /// phase-412 W2 -- the liveliness pool is every token THIS session
    /// declares, term by term against the zenoh shim (`shim/session.rs`).
    #[test]
    fn liveliness_demand_counts_every_token_the_session_declares() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated(
            "p",
            "n",
            &[
                "publisher",
                "sub",
                "timer",
                "service_server",
                "service_client",
                "action_server",
                "action_client",
            ],
        ));
        let k = inv.derive().knobs().expect("derived").clone();
        let session_node = PRIMARY_NODE_LIVELINESS_TOKENS;
        let node_names = 1;
        let publishers = 1 + ACTION_SERVER_PUBLISHERS; // + feedback, status
        let subscribers = 1 + ACTION_CLIENT_SUBSCRIPTIONS; // + feedback
        let servers = 1 + ACTION_SERVER_QUERYABLES;
        let clients = 1 + ACTION_CLIENT_SERVICE_CLIENTS;
        assert_eq!(
            k.max_liveliness,
            session_node + node_names + publishers + subscribers + servers + clients,
            "a timer declares no token; everything else declares exactly one"
        );
        assert_eq!(k.max_liveliness, 15, "the same sum, spelled as a number");

        // Two components are two node names.
        let mut two = EntityInventory::new("test");
        two.insert(stated("a", "one", &["publisher"]));
        two.insert(stated("b", "two", &["sub"]));
        assert_eq!(two.derive().knobs().unwrap().max_liveliness, 1 + 2 + 1 + 1);

        // Never below the session's own token plus the component's node, so a
        // C array fed this value is never zero-length -- the consumer floors
        // anyway (issue 1015), but the demand does not ask it to.
        let mut clock = EntityInventory::new("test");
        clock.insert(stated("a", "clock", &["timer"]));
        assert_eq!(clock.derive().knobs().unwrap().max_liveliness, 2);
        assert!(
            clock
                .to_cmake()
                .contains("set(NROS_DERIVED_MAX_LIVELINESS 2)\n")
        );
        assert!(
            !clock.to_env().contains("LIVELINESS"),
            "the env carrier cannot say whether the model was seen, so it does \
             not carry the liveliness count"
        );
    }

    /// phase-412 W2 -- the runtime's own servers declare tokens too, and the
    /// lifecycle family registers under the executor's node name.
    #[test]
    fn liveliness_counts_the_runtime_servers_the_bringup_declares() {
        let knobs = |features: &str| {
            EntityInventory::from_model("t", &infra_model(features))
                .expect("model describes wiring")
                .derive()
                .knobs()
                .expect("derived")
                .clone()
        };
        let none = knobs("");
        let both = knobs("param_services, lifecycle");
        assert_eq!(
            both.max_liveliness - none.max_liveliness,
            both.infra_queryables + 1,
            "one token per runtime server, plus one node name for lifecycle"
        );
        assert_eq!(
            both.infra_queryables,
            2 * PARAM_SERVICE_QUERYABLES + LIFECYCLE_SERVICE_QUERYABLES
        );
    }

    /// Issue 1130 -- the knob-capped cell registry capacity is the largest
    /// single kind in any ONE component, over the five kinds a cell registers.
    #[test]
    fn cell_bound_is_the_largest_single_kind_in_any_one_component() {
        let mut inv = EntityInventory::new("test");
        inv.insert(stated("a", "talker", &["pub*3", "sub*5", "timer*4"]));
        inv.insert(stated(
            "b",
            "server",
            &["service_server*2", "service_client", "action_server"],
        ));
        let k = inv.derive().knobs().expect("derived").clone();
        assert_eq!(
            k.max_cell_entities, 3,
            "subscriptions and timers reach no cell registry; the largest kind \
             is a::talker's three publishers"
        );

        // Per component, never summed across the image: each cell has its own
        // registries, so two components with two publishers each need 2.
        let mut split = EntityInventory::new("test");
        split.insert(stated("a", "one", &["pub*2"]));
        split.insert(stated("b", "two", &["pub*2"]));
        assert_eq!(split.derive().knobs().unwrap().max_cell_entities, 2);

        // Zero is an ANSWER and travels as one, unfloored.
        let mut listener = EntityInventory::new("test");
        listener.insert(stated("a", "listener", &["sub", "timer"]));
        assert_eq!(listener.derive().knobs().unwrap().max_cell_entities, 0);
        assert!(
            listener
                .to_cmake()
                .contains("set(NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES 0)\n")
        );
        assert!(
            listener
                .to_env()
                .contains("NROS_RUNTIME_MAX_CELL_ENTITIES=0\n")
        );

        // One undeclared component and no transport carries either number.
        listener.insert(ComponentEntities {
            pkg: "b".into(),
            component: "two".into(),
            class: "b::Two".into(),
            declaration: Declaration::Absent,
        });
        let cmake = listener.to_cmake();
        assert!(!cmake.contains("NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES"));
        assert!(!cmake.contains("NROS_DERIVED_MAX_LIVELINESS"));
        assert_eq!(listener.to_env(), "");
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
    /// Issue 1270 -- a two-node model with one application service server,
    /// with `features` as the bringup's `execution.features`.
    fn infra_model(features: &str) -> ros_launch_manifest_model::SystemModel {
        let yaml = format!(
            r#"
meta:
  version: 1
structure:
  nodes:
    /a: {{ scope: s.launch.xml, pkg: p, exec: a, node_name: a }}
    /b: {{ scope: s.launch.xml, pkg: p, exec: b, node_name: b }}
  services:
    /add:
      type: example_interfaces/srv/AddTwoInts
      server: [/a/add]
  topics:
    /chatter:
      type: std_msgs/msg/String
      pub: [/b/chatter]
execution:
  features: [{features}]
"#
        );
        serde_yaml_ng::from_str(&yaml).expect("model fixture parses")
    }

    /// Issue 1270 -- `param_services` in the bringup is six queryables PER
    /// NODE in the session pool, and `lifecycle` five once. Before this the
    /// inventory left both out, so every image declaring them derived a pool
    /// short by exactly those servers and failed registration at boot.
    #[test]
    fn declared_infrastructure_services_are_counted_into_the_queryable_pool() {
        let knobs = |features: &str| {
            EntityInventory::from_model("t", &infra_model(features))
                .expect("model describes wiring")
                .derive()
                .knobs()
                .expect("derived")
                .clone()
        };
        let none = knobs("");
        assert_eq!(none.max_queryables, 1, "the application's one server");
        assert_eq!(none.infra_queryables, 0);
        assert_eq!(none.param_service_nodes, 0);

        let params = knobs("param_services");
        assert_eq!(params.param_service_nodes, 2, "one set of six per node");
        assert_eq!(params.max_queryables, 1 + 2 * PARAM_SERVICE_QUERYABLES);

        let both = knobs("param_services, lifecycle");
        assert_eq!(
            both.max_queryables,
            1 + 2 * PARAM_SERVICE_QUERYABLES + LIFECYCLE_SERVICE_QUERYABLES
        );
        assert_eq!(
            both.max_cbs, none.max_cbs,
            "both families live outside the executor arena: no callback slot"
        );

        // `safety` is a real feature and not a queryable question.
        assert_eq!(knobs("safety").max_queryables, 1);
    }

    /// Issue 1270 -- the configure's inventory is metadata MERGED with the
    /// model, and metadata carries no bringup features: the model's
    /// declaration must survive the merge and reach the CMake knob.
    #[test]
    fn the_merge_keeps_the_families_the_model_declares() {
        let model_inv = EntityInventory::from_model("model", &infra_model("param_services"))
            .expect("model describes wiring");
        let mut meta = EntityInventory::new("meta");
        meta.insert(stated("p", "a", &["timer"]));
        let merged = meta.merged_per_kind_max(&model_inv);
        let k = merged.derive().knobs().expect("derived").clone();
        assert_eq!(k.infra_queryables, 2 * PARAM_SERVICE_QUERYABLES);
        let cmake = merged.to_cmake();
        assert!(
            cmake.contains(&format!(
                "set(NROS_DERIVED_MAX_QUERYABLES {})\n",
                1 + 2 * PARAM_SERVICE_QUERYABLES
            )),
            "{cmake}"
        );
        assert!(
            cmake.contains("of which 12 are the runtime's own servers"),
            "the runtime's share is attributed: {cmake}"
        );
    }

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

#[cfg(test)]
mod from_model_tests {
    use super::*;
    use ros_launch_manifest_model::SystemModel;

    /// phase-412 -- the island's own resolved model, cut down to the shape that
    /// matters: two nodes, three topics, one of them internal.
    ///
    /// Asserts the counts the pool derivation consumes, not the parse: the
    /// question is whether `structure.topics` yields the same per-node sub/pub
    /// sets the hand-written `ENTITIES` lists did.
    fn model_from_yaml(y: &str) -> SystemModel {
        serde_yaml_ng::from_str(y).expect("model fixture parses")
    }

    /// A model running `nodes`, whose contract declares `params` per node.
    /// A node listed in `nodes` and absent from `params` has no entry (it
    /// said nothing); one listed with no names gets `{}`, the empty entry the
    /// resolver writes for `params: {}` (phase-446 F1).
    fn param_model(nodes: &[&str], params: &[(&str, &[(&str, &str)])]) -> SystemModel {
        let mut y = String::from("meta: { version: 1 }\nstructure:\n  nodes:\n");
        for n in nodes {
            let short = n.rsplit('/').next().unwrap();
            y.push_str(&format!(
                "    {n}: {{ scope: s.launch.xml, pkg: p, exec: {short}, node_name: {short} }}\n"
            ));
        }
        if !params.is_empty() {
            y.push_str("contracts:\n  node_params:\n");
            for (node, ps) in params {
                if ps.is_empty() {
                    y.push_str(&format!("    {node}: {{}}\n"));
                    continue;
                }
                y.push_str(&format!("    {node}:\n"));
                for (name, ty) in *ps {
                    y.push_str(&format!("      {name}: {{ type: {ty} }}\n"));
                }
            }
        }
        model_from_yaml(&y)
    }

    /// The downstream island in the shape phase-446 W4 was measured on: four
    /// component nodes, 21 scalar parameters, the longest name 35 bytes.
    const ISLAND_NODES: [&str; 4] = [
        "/system/mrm_handler",
        "/system/stop_mode_operator",
        "/system/leader_election",
        "/system/diag_aggregator",
    ];
    const MRM: &[(&str, &str)] = &[
        ("update_rate", "integer"),
        ("timeout_operation_mode_availability", "double"),
        ("use_emergency_holding", "bool"),
        ("turning_hazard_on.emergency", "bool"),
        ("timeout_emergency_recovery", "double"),
        ("use_parking_after_stopped", "bool"),
        ("use_pull_over", "bool"),
    ];
    const STOP: &[(&str, &str)] = &[
        ("stop_hold_acceleration", "double"),
        ("enable_auto_parking", "bool"),
        ("timeout_sec", "double"),
        ("publish_rate", "integer"),
        ("velocity_threshold", "double"),
        ("use_brake", "bool"),
    ];
    const LEADER: &[(&str, &str)] = &[
        ("heartbeat_period", "double"),
        ("election_timeout", "double"),
        ("node_id", "integer"),
        ("peers_count", "integer"),
        ("verbose", "bool"),
    ];
    const DIAG: &[(&str, &str)] = &[
        ("period", "double"),
        ("min_level", "integer"),
        ("use_emergency", "bool"),
    ];

    fn island_params() -> Vec<(&'static str, &'static [(&'static str, &'static str)])> {
        vec![
            (ISLAND_NODES[0], MRM),
            (ISLAND_NODES[1], STOP),
            (ISLAND_NODES[2], LEADER),
            (ISLAND_NODES[3], DIAG),
        ]
    }

    fn with_params(m: &SystemModel) -> EntityInventory {
        let mut inv = EntityInventory::new("test");
        inv.set_param_declarations(ParamDeclarations::from_model(m));
        inv
    }

    /// phase-446 W4 acceptance -- 21 declared scalars on four nodes derive 25
    /// slots (one seeded `use_sim_time` per node), a 35-byte name bound, and
    /// ZERO for every capacity, because no node declares a string or an array.
    /// At those limits `ParameterStorage<25>` measured 4,200 bytes against
    /// 285,440 for the default 32 slots.
    #[test]
    fn four_nodes_and_twenty_one_scalars_size_the_store_to_twenty_five_slots() {
        let m = param_model(&ISLAND_NODES, &island_params());
        let z = ParamDeclarations::from_model(&m)
            .sizing()
            .expect("every node declares, so the store is sized");
        assert_eq!(z.declared, 21);
        assert_eq!(z.max_parameters, 25, "21 declared + 4 seeded use_sim_time");
        assert_eq!(
            z.max_param_name_len, 35,
            "timeout_operation_mode_availability"
        );
        for (knob, cap) in z.capacities() {
            assert_eq!(cap, &ParamCapacity::Unused, "{knob}: no node uses it");
        }
        let c = with_params(&m).to_cmake();
        for line in [
            "set(NROS_PARAM_DECLARATION_STATUS \"declared\")\n",
            "set(NROS_PARAM_DECLARED_COUNT 21)\n",
            "set(NROS_DERIVED_MAX_PARAMETERS 25)\n",
            "set(NROS_DERIVED_MAX_PARAM_NAME_LEN 35)\n",
            "set(NROS_DERIVED_MAX_STRING_VALUE_LEN 0)\n",
            "set(NROS_DERIVED_MAX_ARRAY_LEN 0)\n",
            "set(NROS_DERIVED_MAX_BYTE_ARRAY_LEN 0)\n",
        ] {
            assert!(c.contains(line), "missing {line:?} in:\n{c}");
        }
        assert!(!c.contains("NROS_PARAM_NEEDS_"), "{c}");
    }

    /// A declared string gets NO number from the contract: the fragment names
    /// the parameter and the knob, and the board supplies the size or the
    /// `nros-params` build refuses. The other capacities stay independent.
    #[test]
    fn a_declared_string_asks_the_board_and_derives_no_capacity() {
        let mut ps = island_params();
        let extra: &[(&str, &str)] = &[("greeting", "string"), ("rate", "integer")];
        ps[3] = (ISLAND_NODES[3], extra);
        let m = param_model(&ISLAND_NODES, &ps);
        let z = ParamDeclarations::from_model(&m).sizing().unwrap();
        match &z.string_value_len {
            ParamCapacity::NeededBy(p) => {
                assert_eq!(p.token(), "/system/diag_aggregator:greeting:string");
            }
            other => panic!("a string is declared, got {other:?}"),
        }
        assert_eq!(z.array_len, ParamCapacity::Unused);
        assert_eq!(z.byte_array_len, ParamCapacity::Unused);
        let c = with_params(&m).to_cmake();
        assert!(
            c.contains(
                "set(NROS_PARAM_NEEDS_MAX_STRING_VALUE_LEN \
                 \"/system/diag_aggregator:greeting:string\")\n"
            ),
            "{c}"
        );
        assert!(!c.contains("NROS_DERIVED_MAX_STRING_VALUE_LEN"), "{c}");
        assert!(c.contains("set(NROS_DERIVED_MAX_ARRAY_LEN 0)\n"), "{c}");
    }

    /// Each capacity answers to its own types: `string_array` needs a string
    /// length AND an array length, `byte_array` only the byte-array length.
    #[test]
    fn each_array_type_asks_for_the_capacities_it_uses() {
        let a: &[(&str, &str)] = &[("names", "string_array")];
        let b: &[(&str, &str)] = &[("blob", "byte_array")];
        let m = param_model(&["/a", "/b"], &[("/a", a), ("/b", b)]);
        let z = ParamDeclarations::from_model(&m).sizing().unwrap();
        assert!(matches!(z.string_value_len, ParamCapacity::NeededBy(ref p) if p.name == "names"));
        assert!(matches!(z.array_len, ParamCapacity::NeededBy(ref p) if p.name == "names"));
        assert!(matches!(z.byte_array_len, ParamCapacity::NeededBy(ref p) if p.name == "blob"));

        let i: &[(&str, &str)] = &[("gains", "double_array")];
        let m = param_model(&["/a"], &[("/a", i)]);
        let z = ParamDeclarations::from_model(&m).sizing().unwrap();
        assert_eq!(z.string_value_len, ParamCapacity::Unused);
        assert!(matches!(z.array_len, ParamCapacity::NeededBy(_)));
        assert_eq!(z.byte_array_len, ParamCapacity::Unused);
    }

    /// Absence is not zero: a contract that declares no parameters leaves
    /// every store knob to its configured value.
    #[test]
    fn a_contract_without_params_derives_nothing() {
        let m = param_model(&ISLAND_NODES, &[]);
        assert_eq!(ParamDeclarations::from_model(&m), ParamDeclarations::Absent);
        let c = with_params(&m).to_cmake();
        assert!(
            c.contains("set(NROS_PARAM_DECLARATION_STATUS \"absent\")\n"),
            "{c}"
        );
        assert!(!c.contains("NROS_DERIVED_MAX_PARAM"), "{c}");
        assert!(!c.contains("NROS_DERIVED_MAX_STRING_VALUE_LEN"), "{c}");
    }

    /// One node that declares nothing makes the whole store unsized: its code
    /// may declare anything, and a count over the other nodes is short.
    #[test]
    fn a_node_that_declares_no_params_refuses_the_store() {
        let ps = island_params();
        let m = param_model(&ISLAND_NODES, &ps[..3]);
        let d = ParamDeclarations::from_model(&m);
        match &d {
            ParamDeclarations::Refused { reason } => {
                assert!(reason.contains("/system/diag_aggregator"), "{reason}");
            }
            other => panic!("a silent node must refuse, got {other:?}"),
        }
        assert_eq!(d.sizing(), None);
        let c = with_params(&m).to_cmake();
        assert!(
            c.contains("set(NROS_PARAM_DECLARATION_STATUS \"refused\")\n"),
            "{c}"
        );
        assert!(!c.contains("NROS_DERIVED_MAX_PARAMETERS"), "{c}");
    }

    /// phase-446 F1 -- an EMPTY entry is a declaration of none, not silence.
    /// The same image with `/system/diag_aggregator: {}` is sized, and that
    /// node gets only its seeded `use_sim_time`; drop the entry and the image
    /// refuses again, naming it. (`tests/param_declarations_resolve.rs` makes
    /// the same pair through the pinned resolver.)
    #[test]
    fn an_empty_params_entry_declares_none_and_a_missing_one_refuses() {
        let none: &[(&str, &str)] = &[];
        let mut ps = island_params();
        ps[3] = (ISLAND_NODES[3], none);
        let m = param_model(&ISLAND_NODES, &ps);
        assert_eq!(
            m.contracts
                .node_params
                .get(ISLAND_NODES[3])
                .map(|p| p.len()),
            Some(0),
            "the fixture carries the empty entry"
        );
        let d = ParamDeclarations::from_model(&m);
        let ParamDeclarations::Declared { nodes, params } = &d else {
            panic!("an empty entry counts as declared, got {d:?}");
        };
        assert!(nodes.iter().any(|n| n == ISLAND_NODES[3]), "{nodes:?}");
        assert!(
            params.iter().all(|p| p.node != ISLAND_NODES[3]),
            "{params:?}"
        );
        let z = d
            .sizing()
            .expect("every node declares, so the store is sized");
        assert_eq!(z.declared, 18, "21 less the three DIAG names");
        assert_eq!(
            z.max_parameters, 22,
            "18 declared + 4 seeded; the empty node's one slot is its seed"
        );
        let c = with_params(&m).to_cmake();
        assert!(
            c.contains("set(NROS_PARAM_DECLARATION_STATUS \"declared\")\n"),
            "{c}"
        );
        assert!(c.contains("set(NROS_DERIVED_MAX_PARAMETERS 22)\n"), "{c}");

        let m = param_model(&ISLAND_NODES, &ps[..3]);
        match ParamDeclarations::from_model(&m) {
            ParamDeclarations::Refused { reason } => {
                assert!(reason.contains(ISLAND_NODES[3]), "{reason}");
                assert!(reason.starts_with("1 of 4 nodes"), "{reason}");
            }
            other => panic!("no entry must refuse, got {other:?}"),
        }
    }

    /// A contract that names `use_sim_time` itself does not get a second
    /// slot: the seed steps aside for the node's own declaration.
    #[test]
    fn a_declared_use_sim_time_shares_the_seeded_slot() {
        let a: &[(&str, &str)] = &[("use_sim_time", "bool"), ("rate", "integer")];
        let m = param_model(&["/a"], &[("/a", a)]);
        let z = ParamDeclarations::from_model(&m).sizing().unwrap();
        assert_eq!(z.declared, 2);
        assert_eq!(z.max_parameters, 2);
        assert_eq!(z.max_param_name_len, "use_sim_time".len());
    }

    /// The model is the only source of parameter declarations, so the merge
    /// with a metadata declaration keeps the model's.
    #[test]
    fn the_merge_keeps_the_models_parameter_declarations() {
        let m = param_model(&ISLAND_NODES, &island_params());
        let model_inv = with_params(&m);
        let merged = EntityInventory::new("metadata").merged_per_kind_max(&model_inv);
        assert_eq!(
            merged
                .param_declarations()
                .sizing()
                .map(|z| z.max_parameters),
            Some(25)
        );
    }

    /// phase-446 F3 -- the parameter services' half of the declaration, one
    /// shape per node in node order. The island fixture is all scalars, so
    /// only the counts, the name bytes and `mrm_handler`'s one dotted prefix
    /// (`turning_hazard_on`) move; `use_sim_time` is counted once per node.
    #[test]
    fn each_node_carries_its_parameter_service_shape() {
        let m = param_model(&ISLAND_NODES, &island_params());
        let d = ParamDeclarations::from_model(&m);
        let shapes = d.service_shapes().expect("every node declares");
        // `/system/diag_aggregator`, `leader_election`, `mrm_handler`,
        // `stop_mode_operator`: 3 + 5 + 7 + 6 declared, each + use_sim_time.
        let token = "4:40:0:0:0:0:0:0:0,6:69:0:0:0:0:0:0:0,8:170:1:17:0:0:0:0:0,\
                     7:103:0:0:0:0:0:0:0";
        assert_eq!(ParamServiceShape::token(&shapes), token);
        let c = with_params(&m).to_cmake();
        assert!(
            c.contains(&format!("set(NROS_PARAM_SERVICE_SHAPE \"{token}\")\n")),
            "{c}"
        );
        assert!(with_params(&m).to_json().contains(token));
    }

    /// Every type lands in its own counter -- integer and double arrays
    /// share one, being the same 8-byte words on the wire -- and a declared
    /// `use_sim_time` is not counted twice.
    #[test]
    fn each_parameter_type_lands_in_its_service_shape_counter() {
        let a: &[(&str, &str)] = &[
            ("names", "string_array"),
            ("blob", "byte_array"),
            ("gains", "double_array"),
            ("ids", "integer_array"),
            ("flags", "bool_array"),
            ("label", "string"),
            ("ctl.kp", "double"),
            ("ctl.ki", "double"),
            ("use_sim_time", "bool"),
        ];
        let m = param_model(&["/a"], &[("/a", a)]);
        let shapes = ParamDeclarations::from_model(&m).service_shapes().unwrap();
        assert_eq!(
            shapes,
            vec![ParamServiceShape {
                params: 9,
                name_bytes: 5 + 4 + 5 + 3 + 5 + 5 + 6 + 6 + 12,
                prefixes: 1,
                prefix_bytes: 3,
                strings: 1,
                byte_arrays: 1,
                bool_arrays: 1,
                word_arrays: 2,
                string_arrays: 1,
            }]
        );
        assert_eq!(ParamServiceShape::token(&shapes), "9:51:1:3:1:1:1:2:1");
    }

    /// Absence is not a shape: no `params:` anywhere, or a node that states
    /// none, carries no service shape, so nros-node keeps its configured size.
    #[test]
    fn no_declaration_carries_no_service_shape() {
        let absent = param_model(&ISLAND_NODES, &[]);
        let refused = param_model(&ISLAND_NODES, &island_params()[..3]);
        for m in [absent, refused] {
            assert_eq!(ParamDeclarations::from_model(&m).service_shapes(), None);
            let c = with_params(&m).to_cmake();
            assert!(!c.contains("NROS_PARAM_SERVICE_SHAPE"), "{c}");
        }
    }

    #[test]
    fn topics_become_per_node_subscriptions_and_publishers() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /mrm_handler:
      { scope: s.launch.xml, pkg: autoware_mrm_handler, exec: mrm_handler,
        node_name: mrm_handler }
    /stop_mode_operator:
      { scope: s.launch.xml, pkg: autoware_stop_mode_operator,
        exec: stop_mode_operator, node_name: stop_mode_operator }
  topics:
    /system/mrm/emergency_stop/status:
      type: tier4_system_msgs/msg/MrmBehaviorStatus
      sub: [/mrm_handler/emergency_stop_status]
    /api/operation_mode/state:
      type: autoware_adapi_v1_msgs/msg/OperationModeState
      sub: [/mrm_handler/operation_mode_state]
    /system/stop_mode/control:
      type: autoware_control_msgs/msg/Control
      pub: [/stop_mode_operator/control]
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let d = inv.derive();
        let k = d.knobs().expect("wiring yields knobs");
        assert_eq!(k.max_subscribers, 2, "two subscriptions across the image");
        assert_eq!(k.max_publishers, 1, "one publisher");
        assert_eq!(
            k.max_queryables, 0,
            "no service server is declared, so the DEMAND is zero"
        );
    }

    /// Issues 1015 + 1033 — the derivation publishes DEMAND, and the floor
    /// belongs to whichever consumer's storage cannot be empty.
    ///
    /// A publisher-only image demands zero subscribers and zero queryables.
    /// Flooring that here reaches BOTH consumers of the number: it stopped a
    /// zenoh board transmitting at 0 (1015) and it silently re-charged an XRCE
    /// image 33,296 bytes for a subscriber slot it does not have and 4,384 for
    /// a service-server slot (1033, measured from the listener's DWARF). One of
    /// those two is right at any given moment and the other is wrong, which is
    /// the argument for keeping this number honest and flooring at the pools.
    #[test]
    fn a_publisher_only_image_derives_a_demand_of_zero() {
        let mut inv = EntityInventory::new("test");
        inv.insert(ComponentEntities {
            pkg: "p".into(),
            component: "talker".into(),
            class: "Talker".into(),
            declaration: Declaration::Stated(vec![EntityDecl::bare(
                EntityKind::Publisher,
                Some("std_msgs/msg/String".into()),
                Some("/chatter".into()),
            )]),
        });
        let d = inv.derive();
        let k = d.knobs().expect("a stated declaration derives");
        assert_eq!(k.max_publishers, 1);
        assert_eq!(k.max_subscribers, 0, "declares none, so demands none");
        assert_eq!(k.max_queryables, 0, "declares none, so demands none");
        // And the floor a C-array consumer applies to that demand.
        assert_eq!(c_array_pool_floor(k.max_subscribers), 1);
        assert_eq!(c_array_pool_floor(3), 3, "a real demand passes through");
    }

    /// phase-412 -- the merge takes the larger list of each kind, whichever
    /// source it came from.
    ///
    /// The fixture is the island's failure in miniature: a hand-written list
    /// saying one subscription where the contract says two. It also has the
    /// declaration carrying a timer the model row does not, which is what the
    /// per-kind rule protects -- NOT because a contract cannot state a timer
    /// (it can, as a `paths:` entry with a `timer` trigger, which is how
    /// ENTITIES was retired), but because either source may be the one that
    /// knows about a given kind and a whole-component max would let one hide
    /// the other.
    #[test]
    fn the_merge_takes_the_larger_of_each_kind() {
        let mut decl = EntityInventory::new("metadata");
        decl.insert(ComponentEntities {
            pkg: "autoware_mrm_handler".into(),
            component: "mrm_handler".into(),
            class: "MrmHandler".into(),
            declaration: Declaration::Stated(vec![
                EntityDecl::bare(
                    EntityKind::Subscription,
                    Some("a/msg/A".into()),
                    Some("/one".into()),
                ),
                EntityDecl::bare(EntityKind::Timer, None, None),
            ]),
        });

        let mut model = EntityInventory::new("model");
        model.insert(ComponentEntities {
            pkg: "/mrm_handler".into(),
            component: "mrm_handler".into(),
            class: String::new(),
            declaration: Declaration::Stated(vec![
                EntityDecl::bare(
                    EntityKind::Subscription,
                    Some("a/msg/A".into()),
                    Some("/one".into()),
                ),
                EntityDecl::bare(
                    EntityKind::Subscription,
                    Some("b/msg/B".into()),
                    Some("/two".into()),
                ),
            ]),
        });

        let merged = decl.merged_per_kind_max(&model);
        let d = merged.derive();
        let k = d.knobs().expect("merged yields knobs");
        assert_eq!(
            k.max_subscribers, 2,
            "the model's extra subscription survives"
        );
        assert_eq!(
            k.max_cbs, 3,
            "two subscriptions plus the timer only the declaration knows"
        );
    }

    /// phase-412 -- services and actions become server/client entities.
    ///
    /// Both live in `structure` under one `ServiceWiring` shape, and the only
    /// thing that tells a service from an action is which map it came from --
    /// so a fixture with one of each is the smallest thing that can catch the
    /// two maps being read into the same kind.
    #[test]
    fn services_and_actions_become_server_and_client_entities() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /add_server:
      { scope: s.launch.xml, pkg: service_server_pkg, exec: add_server,
        node_name: add_server }
    /fib_client:
      { scope: s.launch.xml, pkg: action_client_pkg, exec: fib_client,
        node_name: fib_client }
  services:
    /add_two_ints:
      type: example_interfaces/srv/AddTwoInts
      server: [/add_server/add_two_ints]
  actions:
    /fibonacci:
      type: example_interfaces/action/Fibonacci
      client: [/fib_client/fibonacci]
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let d = inv.derive();
        let k = d.knobs().expect("wiring yields knobs");
        let n = |tag: &str| k.per_kind.get(tag).copied().unwrap_or(0);
        assert_eq!(
            n(EntityKind::ServiceServer.tag()),
            1,
            "the service's server side"
        );
        assert_eq!(
            n(EntityKind::ActionClient.tag()),
            1,
            "the action's client side"
        );
        assert_eq!(
            n(EntityKind::ServiceClient.tag()),
            0,
            "nothing invented on the side the model left empty"
        );
        assert_eq!(
            n(EntityKind::ActionServer.tag()),
            0,
            "same, the other way round"
        );
    }

    /// phase-412 -- a node path with NO INPUT is the periodic callback.
    ///
    /// This is what retired `ENTITIES`: the claim that the model has no timer
    /// entity was wrong. `PathContract::input` is documented as "empty =
    /// periodic (timer-driven)", and a contract's
    /// `trigger: { timer: { rate_hz: N } }` resolves to exactly that. A path
    /// WITH inputs is a take-and-publish path, already counted through its
    /// subscriptions, and must not add a slot.
    #[test]
    fn a_node_path_with_no_input_is_a_timer_and_one_with_inputs_is_not() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
contracts:
  node_paths:
    /talker/on_timer:
      output: [/talker/chatter]
    /talker/on_message:
      input: [/talker/inbox]
      output: [/talker/chatter]
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let d = inv.derive();
        let k = d.knobs().expect("wiring yields knobs");
        assert_eq!(
            k.per_kind
                .get(EntityKind::Timer.tag())
                .copied()
                .unwrap_or(0),
            1,
            "only the input-less path is a timer"
        );
    }

    /// phase-412 -- an image whose ONLY wiring is a timer still describes
    /// wiring.
    ///
    /// Before `node_paths` joined the emptiness test, such a model returned
    /// `None` -- "nobody authored a contract" -- and the image silently fell
    /// back to its configured pool sizes. That is the same absent-is-not-zero
    /// collapse this module exists to prevent, one layer up.
    #[test]
    fn a_model_with_only_timers_is_not_mistaken_for_an_unauthored_one() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /ticker:
      { scope: s.launch.xml, pkg: ticker_pkg, exec: ticker, node_name: ticker }
contracts:
  node_paths:
    /ticker/on_timer:
      output: []
"#,
        );
        let inv = EntityInventory::from_model("test", &m)
            .expect("a timer-only contract still describes wiring");
        let d = inv.derive();
        let k = d.knobs().expect("wiring yields knobs");
        assert_eq!(
            k.per_kind
                .get(EntityKind::Timer.tag())
                .copied()
                .unwrap_or(0),
            1
        );
    }

    /// phase-412 -- a declared history depth rides the model to the inventory,
    /// and an endpoint that declares none yields `None` rather than `0`.
    ///
    /// A depth of zero is a QoS no subscriber can have, so it must never be
    /// how "not declared" is spelled -- a consumer that read it as a number
    /// would size a queue to nothing.
    #[test]
    fn a_declared_depth_reaches_the_inventory_and_an_undeclared_one_stays_absent() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /deep:
      type: std_msgs/msg/Int32
      sub: [/listener/deep]
    /plain:
      type: std_msgs/msg/Int32
      sub: [/listener/plain]
contracts:
  sub_endpoints:
    /listener/deep:
      qos: { depth: 20 }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let row = inv
            .components()
            .into_iter()
            .find(|c| c.component == "listener")
            .expect("the listener row");
        let mut by_name: Vec<(Option<String>, Option<u32>)> = row
            .declaration
            .entities()
            .iter()
            .filter(|e| e.kind == EntityKind::Subscription)
            .map(|e| (e.name.clone(), e.depth))
            .collect();
        by_name.sort();
        // The row is keyed by the TOPIC, not the endpoint ref that carried the
        // depth (issue 1084). `/listener/deep` is how the contract ADDRESSES
        // the endpoint; `/deep` is what the subscribing call site writes, and a
        // table keyed on the former matches nothing a compiler ever sees.
        assert_eq!(
            by_name,
            vec![
                (Some("/deep".to_string()), Some(20)),
                (Some("/plain".to_string()), None),
            ]
        );
    }

    /// A model whose one topic has a declaring publisher, a declaring
    /// subscriber and a silent one of each -- the shape every depth test below
    /// needs.
    fn model_with_publisher_depths() -> ros_launch_manifest_model::SystemModel {
        model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
    /quiet:
      type: std_msgs/msg/Int32
      pub: [/talker/quiet]
      sub: [/listener/quiet]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { depth: 8 }
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 3 }
"#,
        )
    }

    /// phase-454 W2 -- a PUBLISHER's declared depth reaches the inventory.
    ///
    /// `from_model` hardcoded `depth: None` on every publisher row, so a
    /// contract stating `pub: { /chatter: { qos: { depth: 8 } } }` parsed into
    /// `PubContract::qos.depth`, was never read, and the build saw an image
    /// whose publishers had declared nothing. The grammar had admitted the
    /// field since phase-403 step 2 -- `carries_qos_depth` returns true for a
    /// publisher -- so this was a declaration that was legal to write and
    /// silently dropped.
    #[test]
    fn a_publishers_declared_depth_reaches_the_inventory() {
        let inv = EntityInventory::from_model("test", &model_with_publisher_depths())
            .expect("model describes wiring");
        let mut rows: Vec<(&str, Option<String>, Option<u32>)> = inv
            .components()
            .iter()
            .flat_map(|c| c.declaration.entities())
            .filter(|e| e.kind == EntityKind::Publisher)
            .map(|e| (e.kind.tag(), e.name.clone(), e.depth))
            .collect();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                ("publisher", Some("/chatter".to_string()), Some(8)),
                // Still `None` and never `0`: the publisher that stated nothing
                // has not opted in, which is a different claim from depth 0.
                ("publisher", Some("/quiet".to_string()), None),
            ]
        );
    }

    /// phase-454 W2 -- and it reaches the DECLARED-DEPTH table, tagged.
    ///
    /// The row carries its kind, so the two consumers that price one kind can
    /// narrow. A publisher and a subscription on ONE topic are two rows that
    /// agree on `(type, topic)`, which is why the sort key gained the kind.
    #[test]
    fn the_depth_table_carries_a_publisher_row_beside_the_subscription_row() {
        let inv = EntityInventory::from_model("test", &model_with_publisher_depths())
            .expect("model describes wiring");
        let DeclaredDepths::Resolved {
            rows,
            undeclared,
            undeclared_subscriptions,
            undeclared_publishers,
        } = inv.declared_depths()
        else {
            panic!("a composed inventory resolves its depths");
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
        // One silent publisher and one silent subscriber. The broad count is
        // their sum and answers NEITHER term -- each prices one kind.
        assert_eq!(undeclared, 2);
        assert_eq!(undeclared_subscriptions, 1);
        assert_eq!(undeclared_publishers, 1);
    }

    /// phase-454 W2 -- and it does NOT reach `NROS_ENTITY_DECLARED_DEPTHS`.
    ///
    /// THE regression test for this wave. `nros-node/build.rs::subs_arena`
    /// refuses to size unless `declared.len()` equals the SUBSCRIPTION count,
    /// so a publisher triple appended to that list breaks the equality for
    /// every image that declares both kinds -- and the failure is not an error
    /// but a silent fall back to `subs * pubsub_entry_at_default`, measured at
    /// 207,096 bytes of arena against 71,664 on the reference island. The
    /// publisher depth therefore rides its own variable, which nothing that
    /// sizes a subscription reads.
    #[test]
    fn a_publisher_depth_stays_out_of_the_subscription_sizing_list() {
        let inv = EntityInventory::from_model("test", &model_with_publisher_depths())
            .expect("model describes wiring");
        let cmake = inv.to_cmake();
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_DEPTHS \"std_msgs/msg/Int32|/chatter=3\")\n"),
            "the legacy list is the SUBSCRIPTION depths and nothing else: {cmake}"
        );
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_DEPTH_COUNT 1)\n"),
            "its count must match its own list, not the whole table: {cmake}"
        );
        assert!(
            cmake.contains(
                "set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER \"std_msgs/msg/Int32|/chatter=8\")\n"
            ),
            "the publisher depth travels, in its own name: {cmake}"
        );
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_DEPTH_COUNT_PUBLISHER 1)\n"),
            "{cmake}"
        );
        assert!(
            cmake.contains("set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_PUBLISHER 1)\n"),
            "a publisher-side consumer refuses on its OWN kind's silence: {cmake}"
        );
    }

    /// phase-454 W2 -- the ARENA does not move when a publisher declares.
    ///
    /// The acceptance, measured rather than asserted by construction: take one
    /// image, read every input `nros-node/build.rs::subs_arena` and
    /// `_nros_qos_depth_env` use to size the subscription arena, then add a
    /// publisher depth to the SAME image and read them again. Every one of
    /// them must be byte-identical -- the publisher's declaration is invisible
    /// to the subscription terms, which is the whole point of the split.
    ///
    /// The set below is the FULL read set of those two consumers, which is why
    /// the broad `NROS_ENTITY_UNDECLARED_DEPTH_COUNT` is NOT in it: this test
    /// first failed on exactly that line (1 -> 0, because the publisher that
    /// declared stopped being silent), which is a true statement about the
    /// image and would have been a false input to a subscription price.
    /// `_nros_qos_depth_env` was the one consumer still guarding on it and now
    /// reads `..._SUBSCRIPTION`, so no publisher fact reaches a subscription
    /// number. Adding the broad count back here would re-assert the coupling
    /// this wave removed.
    ///
    /// The reader below is `subs_arena`'s own parse, transcribed: split the
    /// list on `;`/`,`, take the depth after the LAST `=`, and the guard
    /// `declared.len() == subs`. A test that only compared strings would pass
    /// on a list that still parses into the wrong number of entries.
    #[test]
    fn a_publisher_declaration_moves_no_input_the_subscription_arena_reads() {
        /// Every line of the fragment a subscription-sizing consumer reads.
        fn arena_inputs(model: &ros_launch_manifest_model::SystemModel) -> Vec<String> {
            let cmake = EntityInventory::from_model("test", model)
                .expect("model describes wiring")
                .to_cmake();
            cmake
                .lines()
                .filter(|l| {
                    l.starts_with("set(NROS_ENTITY_DECLARED_DEPTHS ")
                        || l.starts_with("set(NROS_ENTITY_DECLARED_DEPTH_COUNT ")
                        || l.starts_with("set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION ")
                        || l.starts_with("set(NROS_ENTITY_DECLARED_DEPTH_STATUS ")
                })
                .map(str::to_string)
                .collect()
        }

        let silent_publishers = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 3 }
"#,
        );
        let declaring_publishers = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { depth: 8 }
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 3 }
"#,
        );
        let before = arena_inputs(&silent_publishers);
        let after = arena_inputs(&declaring_publishers);
        assert_eq!(
            before, after,
            "a publisher's depth must not move any value a subscription term reads"
        );

        // ...and the guard those inputs feed still holds. One subscription,
        // one parsed triple -- so `subs_arena` sizes from the declaration
        // rather than falling back to the worst case.
        let depths = after
            .iter()
            .find_map(|l| l.strip_prefix("set(NROS_ENTITY_DECLARED_DEPTHS \""))
            .and_then(|l| l.strip_suffix("\")"))
            .expect("the subscription list is published");
        let declared: Vec<(&str, usize)> = depths
            .split([';', ','])
            .filter_map(|t| t.rsplit_once('='))
            .filter_map(|(head, d)| {
                let ty = head.split_once('|').map_or(head, |(ty, _topic)| ty).trim();
                d.trim().parse::<usize>().ok().map(|d| (ty, d))
            })
            .collect();
        assert_eq!(
            declared,
            vec![("std_msgs/msg/Int32", 3)],
            "`declared.len() == subs` is subs_arena's whole guard"
        );
    }

    // -----------------------------------------------------------------
    // phase-454 W3 (issue 1256) -- the other three QoS policies.
    // -----------------------------------------------------------------

    /// One topic, both sides stating all four policies, and a silent pair
    /// beside it.
    fn model_with_four_policies() -> ros_launch_manifest_model::SystemModel {
        model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
    /quiet:
      type: std_msgs/msg/Int32
      pub: [/talker/quiet]
      sub: [/listener/quiet]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos:
        depth: 8
        reliability: reliable
        durability: transient_local
        history: keep_last
  sub_endpoints:
    /listener/chatter:
      qos:
        depth: 3
        reliability: best_effort
        durability: volatile
        history: keep_last
"#,
        )
    }

    /// One entity row as the acceptance below reads it: `(kind, topic, depth,
    /// reliability, durability, history)` -- the FOUR policies plus what
    /// identifies the endpoint. Every field is `Option` because "nobody said"
    /// is the claim this whole module exists to keep distinguishable from a
    /// value.
    type DeclaredRow = (
        &'static str,
        Option<String>,
        Option<u32>,
        Option<QoSReliabilityPolicy>,
        Option<QoSDurabilityPolicy>,
        Option<QoSHistoryPolicy>,
    );

    /// phase-454 W3 -- all four policies reach the entity rows, and the two
    /// sides keep their OWN profiles.
    ///
    /// `from_model` read `qos.depth` and dropped the other three, so a contract
    /// saying `reliability: best_effort` parsed, resolved, and was read by
    /// nobody -- issue 1256. The two endpoints state different profiles on
    /// purpose: a reader that looked in the wrong endpoint map would pass a
    /// fixture where both sides agree.
    #[test]
    fn all_four_declared_policies_reach_the_inventory() {
        let inv = EntityInventory::from_model("test", &model_with_four_policies())
            .expect("model describes wiring");
        let mut rows: Vec<DeclaredRow> = inv
            .components()
            .iter()
            .flat_map(|c| c.declaration.entities())
            .filter(|e| e.kind == EntityKind::Publisher || e.kind == EntityKind::Subscription)
            .map(|e| {
                (
                    e.kind.tag(),
                    e.name.clone(),
                    e.depth,
                    e.reliability,
                    e.durability,
                    e.history,
                )
            })
            .collect();
        rows.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        assert_eq!(
            rows,
            vec![
                (
                    "publisher",
                    Some("/chatter".to_string()),
                    Some(8),
                    Some(QoSReliabilityPolicy::Reliable),
                    Some(QoSDurabilityPolicy::TransientLocal),
                    Some(QoSHistoryPolicy::KeepLast),
                ),
                // ABSENCE IS NOT A VALUE. The silent publisher is `None` on
                // every policy, never `Volatile` and never `Reliable`: those
                // are ROS's defaults for an endpoint that did not choose, and
                // recording one here would make "took the default" and "asked
                // for it" the same row.
                (
                    "publisher",
                    Some("/quiet".to_string()),
                    None,
                    None,
                    None,
                    None
                ),
                (
                    "subscription",
                    Some("/chatter".to_string()),
                    Some(3),
                    Some(QoSReliabilityPolicy::BestEffort),
                    Some(QoSDurabilityPolicy::Volatile),
                    Some(QoSHistoryPolicy::KeepLast),
                ),
                (
                    "subscription",
                    Some("/quiet".to_string()),
                    None,
                    None,
                    None,
                    None,
                ),
            ]
        );
    }

    /// phase-454 W3 -- and into the policy VIEW, with a per-policy, per-kind
    /// count of what stayed silent.
    ///
    /// Three counts and not one, for the reason issue 1227 gave the depth
    /// counts one per kind: an image can state `reliability` on every
    /// subscription and `durability` on none, and a single "some policy is
    /// missing" number would pin the reliability consumer on its worst case for
    /// a gap that is not its own.
    #[test]
    fn the_policy_view_counts_what_stayed_silent_per_policy_and_per_kind() {
        let inv = EntityInventory::from_model("test", &model_with_four_policies())
            .expect("model describes wiring");
        let qos = inv.declared_qos();
        let rows = qos.rows().expect("a composed inventory resolves");
        assert_eq!(
            rows.iter()
                .map(|r| (
                    r.kind.tag(),
                    r.topic.as_str(),
                    r.spelling(QosPolicyKind::Reliability)
                ))
                .collect::<Vec<_>>(),
            vec![
                ("publisher", "/chatter", Some("reliable")),
                ("subscription", "/chatter", Some("best_effort")),
            ],
            "only the endpoints that stated SOMETHING get a row"
        );
        for policy in ALL_QOS_POLICY_KINDS {
            assert_eq!(
                qos.undeclared(*policy, EntityKind::Subscription),
                Some(1),
                "{}",
                policy.tag()
            );
            assert_eq!(
                qos.undeclared(*policy, EntityKind::Publisher),
                Some(1),
                "{}",
                policy.tag()
            );
        }
    }

    /// phase-454 W3 -- a policy declared on only ONE of the three axes leaves
    /// the other two counted as silent.
    ///
    /// The case the three-counts-in-one shape would get wrong, isolated: an
    /// image that states `reliability` everywhere and nothing else can size
    /// XRCE's reliable buffers and must still refuse a transient-local
    /// retention budget.
    #[test]
    fn one_stated_policy_does_not_answer_for_the_other_two() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { reliability: best_effort }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let qos = inv.declared_qos();
        assert_eq!(
            qos.undeclared(QosPolicyKind::Reliability, EntityKind::Subscription),
            Some(0),
            "every subscription stated one, so a reliability consumer may size"
        );
        assert_eq!(
            qos.undeclared(QosPolicyKind::Durability, EntityKind::Subscription),
            Some(1)
        );
        assert_eq!(
            qos.undeclared(QosPolicyKind::History, EntityKind::Subscription),
            Some(1)
        );
        // The row exists because SOMETHING was stated, and it carries only what
        // was: the two absent policies are absent from it, never defaulted.
        let row = &qos.rows().expect("resolved")[0];
        assert_eq!(
            row.spelling(QosPolicyKind::Reliability),
            Some("best_effort")
        );
        assert_eq!(row.spelling(QosPolicyKind::Durability), None);
        assert_eq!(row.spelling(QosPolicyKind::History), None);
        // ...and no depth was stated either, so the depth table still refuses
        // through its own count. Two views, two independent answers.
        let DeclaredDepths::Resolved {
            undeclared_subscriptions,
            ..
        } = inv.declared_depths()
        else {
            panic!("no keep_all here");
        };
        assert_eq!(undeclared_subscriptions, 1);
    }

    /// phase-454 W3 -- `history: keep_all` REFUSES the depth-derived facts, and
    /// the refusal names the endpoint.
    ///
    /// THE defect of this wave. A KEEP_ALL queue has no static bound; the depth
    /// stated beside it was read and the history was not, so the arena budgeted
    /// one sample for a queue DDS will let grow to the resource limits. That is
    /// an UNDER-size, the direction that ships `NodeError::BufferTooSmall` at a
    /// registration the sizing model had passed -- and RFC-0100 D6's only
    /// trigger that can.
    #[test]
    fn a_keep_all_endpoint_refuses_the_depth_table_by_name() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
    /quiet:
      type: std_msgs/msg/Int32
      sub: [/listener/quiet]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 1, history: keep_all }
    /listener/quiet:
      qos: { depth: 5, history: keep_last }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let DeclaredDepths::Refused { reason } = inv.declared_depths() else {
            panic!("keep_all has no static bound, so the depth table must refuse");
        };
        assert!(reason.contains("/chatter"), "names the endpoint: {reason}");
        assert!(reason.contains("keep_all"), "{reason}");
        assert!(
            reason.contains("depth: 1"),
            "and quotes the number that would have been believed: {reason}"
        );
        assert!(
            reason.contains("keep_last"),
            "and names the remedy (RFC-0065 D2): {reason}"
        );

        // No fallback and no partial table. The OTHER subscription's `depth: 5`
        // is a real declaration and it must not survive alone: a table over the
        // endpoints that happen to be priceable sizes an image from a subset of
        // itself, which is the under-report this module exists to prevent.
        let cmake = inv.to_cmake();
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"refused\")\n"),
            "{cmake}"
        );
        assert!(
            !cmake.contains("set(NROS_ENTITY_DECLARED_DEPTHS "),
            "not even an empty list, which reads as \"nobody declared\": {cmake}"
        );
    }

    /// phase-454 W3 -- a `keep_all` with NO depth beside it refuses just the
    /// same.
    ///
    /// The refusal is a property of KEEP_ALL, not of the pair. Without this the
    /// obvious implementation -- "refuse when a depth is stated beside a
    /// keep_all" -- would pass every other test here while leaving the arena to
    /// fall back to the ROS default 10 for an unbounded queue, which is the
    /// same under-size one rung quieter.
    #[test]
    fn a_keep_all_with_no_depth_beside_it_refuses_too() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { history: keep_all }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        assert!(
            matches!(inv.declared_depths(), DeclaredDepths::Refused { .. }),
            "KEEP_ALL has no bound whether or not a depth was written beside it"
        );
    }

    /// phase-454 W3 -- and a PUBLISHER's `keep_all` refuses too.
    ///
    /// Publisher-side history is a real queue with a real cost (it is what
    /// `transient_local` retention is sized from), and W2 made a publisher's
    /// depth travel. A refusal that read only `sub_endpoints` would be issue
    /// 1084's defect one map over, for the third time.
    #[test]
    fn a_publisher_keep_all_refuses_the_depth_table_too() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { depth: 4, history: keep_all }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let DeclaredDepths::Refused { reason } = inv.declared_depths() else {
            panic!("a publisher's keep_all queue has no bound either");
        };
        assert!(reason.contains("publisher"), "{reason}");
        assert!(reason.contains("/chatter"), "{reason}");
    }

    /// phase-454 W3 -- the refusal is PER FACT (RFC-0100 D6) and reaches
    /// nothing else.
    ///
    /// D6's own words: *"a `keep_all` subscription says nothing about Cyclone's
    /// type table, and a global refusal would degrade it anyway."* The entity
    /// counts, the subscribed-type set and the POLICY table all keep resolving
    /// -- including the `keep_all` row itself, because the statement is well
    /// declared and a consumer that reads history (an XRCE `STREAM_HISTORY`, a
    /// Cyclone resource limit) must be able to see it. Only the DEPTH
    /// arithmetic has no answer.
    #[test]
    fn the_keep_all_refusal_is_per_fact_and_degrades_nothing_else() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      sub: [/listener/chatter]
contracts:
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 1, history: keep_all, reliability: best_effort }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        assert!(
            matches!(inv.declared_depths(), DeclaredDepths::Refused { .. }),
            "the depth-derived fact refuses"
        );
        assert!(
            inv.derive().knobs().is_some(),
            "an entity COUNT does not depend on a queue depth"
        );
        assert!(
            inv.subscribed_types().types().is_some(),
            "a payload class is a property of the TYPE"
        );
        let qos = inv.declared_qos();
        let row = &qos.rows().expect("the policy table resolves")[0];
        assert_eq!(row.spelling(QosPolicyKind::History), Some("keep_all"));
        assert_eq!(
            row.spelling(QosPolicyKind::Reliability),
            Some("best_effort")
        );
        let cmake = inv.to_cmake();
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_QOS_STATUS \"resolved\")\n"),
            "{cmake}"
        );
        assert!(
            cmake.contains(
                "set(NROS_ENTITY_DECLARED_HISTORY \
                            \"std_msgs/msg/Int32|/chatter=keep_all\")\n"
            ),
            "{cmake}"
        );
    }

    /// phase-454 W3 -- THE ARENA IS UNCHANGED for an image that declares only
    /// depths.
    ///
    /// The same acceptance W2 measured, against this wave's addition: take one
    /// image, read every input `nros-node/build.rs::subs_arena` and
    /// `_nros_qos_depth_env` use to size the subscription arena, then add the
    /// three policies to the SAME image and read them again. Every line must be
    /// byte-identical.
    ///
    /// Byte-identical is the right bar rather than "the number is the same",
    /// because the two consumers PARSE those lines: `subs_arena` splits the
    /// list on `;`/`,` and takes the depth after the last `=`, and its whole
    /// guard is `declared.len() == subs`. A policy value that leaked into that
    /// list would not change any number here and would silently break the
    /// equality, dropping every declaring image back to
    /// `subs * pubsub_entry_at_default` -- 207,096 bytes of arena against
    /// 71,664 on the reference island.
    ///
    /// The read set below is W2's, unchanged, and deliberately does NOT include
    /// the broad `NROS_ENTITY_UNDECLARED_DEPTH_COUNT`: that is the coupling W2
    /// removed, and re-asserting it here would re-create it.
    #[test]
    fn a_policy_declaration_moves_no_input_the_subscription_arena_reads() {
        fn arena_inputs(model: &ros_launch_manifest_model::SystemModel) -> Vec<String> {
            let cmake = EntityInventory::from_model("test", model)
                .expect("model describes wiring")
                .to_cmake();
            cmake
                .lines()
                .filter(|l| {
                    l.starts_with("set(NROS_ENTITY_DECLARED_DEPTHS ")
                        || l.starts_with("set(NROS_ENTITY_DECLARED_DEPTH_COUNT ")
                        || l.starts_with("set(NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION ")
                        || l.starts_with("set(NROS_ENTITY_DECLARED_DEPTH_STATUS ")
                })
                .map(str::to_string)
                .collect()
        }

        let depths_only = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos: { depth: 8 }
  sub_endpoints:
    /listener/chatter:
      qos: { depth: 3 }
"#,
        );
        let with_policies = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
contracts:
  pub_endpoints:
    /talker/chatter:
      qos:
        depth: 8
        reliability: reliable
        durability: transient_local
        history: keep_last
  sub_endpoints:
    /listener/chatter:
      qos:
        depth: 3
        reliability: best_effort
        durability: volatile
        history: keep_last
"#,
        );
        let before = arena_inputs(&depths_only);
        let after = arena_inputs(&with_policies);
        assert_eq!(
            before, after,
            "stating reliability/durability/history must not move any value a \
             subscription depth term reads"
        );

        // ...and `subs_arena`'s own parse still finds exactly one triple for
        // the one subscription, which is the guard those inputs feed. Its
        // reader, transcribed: split on `;`/`,`, depth after the LAST `=`.
        let depths = after
            .iter()
            .find_map(|l| l.strip_prefix("set(NROS_ENTITY_DECLARED_DEPTHS \""))
            .and_then(|l| l.strip_suffix("\")"))
            .expect("the subscription list is published");
        let declared: Vec<(&str, usize)> = depths
            .split([';', ','])
            .filter_map(|t| t.rsplit_once('='))
            .filter_map(|(head, d)| {
                let ty = head.split_once('|').map_or(head, |(ty, _topic)| ty).trim();
                d.trim().parse::<usize>().ok().map(|d| (ty, d))
            })
            .collect();
        assert_eq!(
            declared,
            vec![("std_msgs/msg/Int32", 3)],
            "`declared.len() == subs` is subs_arena's whole guard, and a policy \
             value in this list would break it without moving a number"
        );
    }

    /// phase-454 W3 -- and the image that declares NOTHING new is byte-identical
    /// in the WHOLE fragment except for what W3 added.
    ///
    /// The stronger half of the arena acceptance: not "the depth lines agree"
    /// but "nothing else moved either". Every line an existing image's fragment
    /// carried before this wave must still be there, verbatim -- so a build that
    /// reads any of them reads the same bytes, and the only difference is the
    /// new block.
    #[test]
    fn an_image_that_declares_no_policy_gains_only_the_new_block() {
        let m = model_with_publisher_depths();
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let cmake = inv.to_cmake();
        let new_block: Vec<&str> = cmake
            .lines()
            .filter(|l| {
                l.contains("DECLARED_QOS_")
                    || l.contains("DECLARED_RELIABILITY")
                    || l.contains("DECLARED_DURABILITY")
                    || l.contains("DECLARED_HISTORY")
                    || l.contains("UNDECLARED_RELIABILITY")
                    || l.contains("UNDECLARED_DURABILITY")
                    || l.contains("UNDECLARED_HISTORY")
            })
            .collect();
        // Six lists and six counts and one status, all of them EMPTY or the
        // full endpoint count: this image states no policy at all.
        assert_eq!(
            new_block,
            vec![
                "set(NROS_ENTITY_DECLARED_QOS_STATUS \"resolved\")",
                "set(NROS_ENTITY_DECLARED_RELIABILITY \"\")",
                "set(NROS_ENTITY_DECLARED_RELIABILITY_PUBLISHER \"\")",
                "set(NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_SUBSCRIPTION 2)",
                "set(NROS_ENTITY_UNDECLARED_RELIABILITY_COUNT_PUBLISHER 2)",
                "set(NROS_ENTITY_DECLARED_DURABILITY \"\")",
                "set(NROS_ENTITY_DECLARED_DURABILITY_PUBLISHER \"\")",
                "set(NROS_ENTITY_UNDECLARED_DURABILITY_COUNT_SUBSCRIPTION 2)",
                "set(NROS_ENTITY_UNDECLARED_DURABILITY_COUNT_PUBLISHER 2)",
                "set(NROS_ENTITY_DECLARED_HISTORY \"\")",
                "set(NROS_ENTITY_DECLARED_HISTORY_PUBLISHER \"\")",
                "set(NROS_ENTITY_UNDECLARED_HISTORY_COUNT_SUBSCRIPTION 2)",
                "set(NROS_ENTITY_UNDECLARED_HISTORY_COUNT_PUBLISHER 2)",
            ],
            "an empty list is NOT the same claim as a missing one -- it says \
             \"this image stated none\", which is exactly what the counts beside \
             it quantify"
        );
        // And every `set(` line that is NOT in the new block is one the schema
        // version aside, a version-4 fragment carried too.
        let carried: Vec<&str> = cmake
            .lines()
            .filter(|l| l.starts_with("set(") && !new_block.contains(l))
            .collect();
        assert!(
            carried.contains(&"set(NROS_ENTITY_DECLARED_DEPTHS \"std_msgs/msg/Int32|/chatter=3\")"),
            "{carried:?}"
        );
        assert!(
            carried.contains(
                &"set(NROS_ENTITY_DECLARED_DEPTHS_PUBLISHER \"std_msgs/msg/Int32|/chatter=8\")"
            ),
            "{carried:?}"
        );
    }

    /// issue 1084 -- the depth table a contract produces is keyed on the string
    /// a `NROS_SUBSCRIBE` call site writes, all the way through the renderer.
    ///
    /// The end-to-end assertion, because every link between the contract and
    /// the compiler is a place the key can change: the contract says
    /// `/mrm_handler/emergency_stop_status`, the code says
    /// `/system/mrm/emergency_stop/status`, and only the second may reach the
    /// generated header.
    #[test]
    fn the_rendered_header_is_keyed_on_the_topic_a_call_site_writes() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /mrm_handler:
      { scope: s.launch.xml, pkg: autoware_mrm_handler, exec: mrm_handler,
        node_name: mrm_handler }
  topics:
    /system/mrm/emergency_stop/status:
      type: tier4_system_msgs/msg/MrmBehaviorStatus
      sub: [/mrm_handler/emergency_stop_status]
contracts:
  sub_endpoints:
    /mrm_handler/emergency_stop_status:
      qos: { depth: 1 }
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let h = inv.to_declared_qos_header();
        assert!(
            h.contains(
                "NROS_DECLARED_QOS_ROW(\"tier4_system_msgs::msg::dds_::MrmBehaviorStatus_\", \
                 \"/system/mrm/emergency_stop/status\", 1)"
            ),
            "the table must be keyed on the topic the code subscribes to: {h}"
        );
        assert!(
            !h.contains("/mrm_handler/emergency_stop_status"),
            "the endpoint ref is how the contract ADDRESSES the endpoint; a row \
             keyed on it matches no call site: {h}"
        );
    }

    /// A component the model does not mention keeps its declaration whole.
    #[test]
    fn a_component_absent_from_the_model_is_not_dropped() {
        let mut decl = EntityInventory::new("metadata");
        decl.insert(ComponentEntities {
            pkg: "p".into(),
            component: "only_declared".into(),
            class: "C".into(),
            declaration: Declaration::Stated(vec![EntityDecl::bare(EntityKind::Timer, None, None)]),
        });
        let merged = decl.merged_per_kind_max(&EntityInventory::new("model"));
        assert_eq!(merged.len(), 1);
        let d = merged.derive();
        assert_eq!(d.knobs().expect("knobs").max_cbs, 1);
    }

    /// A model that describes NO wiring must abstain, never report zero.
    ///
    /// Every launch file in this tree without a contract resolves that way, and
    /// reporting 0 would size each pool to the infrastructure alone and exhaust
    /// it the moment a node registers -- a confident wrong number, which is the
    /// failure shape this module exists to prevent.
    #[test]
    fn a_model_without_wiring_abstains() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: demo, exec: talker, node_name: talker }
"#,
        );
        assert!(
            EntityInventory::from_model("test", &m).is_none(),
            "no contract authored means unanswered, not zero"
        );
    }

    // -----------------------------------------------------------------------
    // phase-454 W8 (RFC-0100 D9) -- `buffer:` earns its keep.
    // -----------------------------------------------------------------------

    /// An image of one subscription, stated field by field.
    ///
    /// Built by hand rather than resolved, and that is a STATEMENT about what
    /// can be resolved: `SubContract` in the pinned `ros-launch-manifest` has no
    /// `buffer` field, so no contract on earth produces a `queue` endpoint in a
    /// SystemModel today (issue 1339, and
    /// `tests/contract_queue_buffer_reaches_the_model.rs` measures it against
    /// the real resolver). These tests exercise the ladder, the arithmetic and
    /// both diagnostics over the rows the reader WILL build the day the field
    /// travels; the rate halves of the same rows are resolved for real, from a
    /// contract, in `the_model_supplies_both_rates_a_queue_default_would_divide`
    /// below.
    fn queue_image(
        buffer: Option<BufferDiscipline>,
        depth: Option<u32>,
        publish_hz: Option<f64>,
        drain_hz: Option<f64>,
    ) -> EntityInventory {
        queue_image_with_history(
            buffer,
            depth,
            publish_hz,
            drain_hz,
            QoSHistoryPolicy::KeepLast,
        )
    }

    fn queue_image_with_history(
        buffer: Option<BufferDiscipline>,
        depth: Option<u32>,
        publish_hz: Option<f64>,
        drain_hz: Option<f64>,
        history: QoSHistoryPolicy,
    ) -> EntityInventory {
        let mut inv = EntityInventory::new("test");
        inv.insert(ComponentEntities {
            pkg: "listener_pkg".into(),
            component: "listener".into(),
            class: String::new(),
            declaration: Declaration::Stated(vec![EntityDecl {
                depth,
                history: Some(history),
                buffer,
                publish_rate: publish_hz.and_then(RateMilliHz::from_hz),
                drain_rate: drain_hz.and_then(RateMilliHz::from_hz),
                ..EntityDecl::bare(
                    EntityKind::Subscription,
                    Some("std_msgs/msg/Int32".into()),
                    Some("/chatter".into()),
                )
            }]),
        });
        inv
    }

    fn only_row(inv: &EntityInventory) -> DeclaredDepth {
        inv.declared_depths()
            .rows()
            .expect("the table resolves")
            .first()
            .expect("the image has one depth-carrying row")
            .clone()
    }

    /// Every `set(` line of an image's fragment, for a byte comparison.
    fn set_lines(inv: &EntityInventory) -> Vec<String> {
        inv.to_cmake()
            .lines()
            .filter(|l| l.starts_with("set("))
            .map(str::to_string)
            .collect()
    }

    /// ACCEPTANCE 1 -- a `queue` endpoint with both rates and no stated depth
    /// gets the derived default, and it reaches the DEPTH TABLE the arena
    /// sizes from.
    ///
    /// 50 Hz in, 10 Hz drained: five arrivals a period plus the straggler slot
    /// the unaligned-window bound forces, so 6. The arithmetic is asserted on
    /// the ROW rather than on `derive_queue_depth` alone, because a correct
    /// function nothing calls is the vacuous shape this campaign keeps finding.
    #[test]
    fn a_queue_endpoint_with_both_rates_gets_the_derived_default() {
        let inv = queue_image(Some(BufferDiscipline::Queue), None, Some(50.0), Some(10.0));
        let row = only_row(&inv);
        assert_eq!(row.depth, 6, "ceil(50 / 10) + 1");
        assert_eq!(
            row.source,
            DepthSource::DerivedFromRates,
            "the row must say it was derived, or a consumer that asserts will assert on it"
        );
        // ...and it is NOT counted as undeclared any more: the endpoint has a
        // depth now, which is the point of a default.
        let DeclaredDepths::Resolved {
            undeclared_subscriptions,
            ..
        } = inv.declared_depths()
        else {
            panic!("the table resolves");
        };
        assert_eq!(undeclared_subscriptions, 0);
        let cmake = inv.to_cmake();
        assert!(
            cmake.contains("set(NROS_ENTITY_DECLARED_DEPTHS \"std_msgs/msg/Int32|/chatter=6\")\n"),
            "{cmake}"
        );
        assert!(
            cmake.contains("set(NROS_ENTITY_DERIVED_DEPTHS \"std_msgs/msg/Int32|/chatter=6\")\n"),
            "the provenance is published beside it: {cmake}"
        );
    }

    /// ACCEPTANCE 2 -- a stated depth beats the derived default, ALL THE WAY
    /// THROUGH.
    ///
    /// The unit test in `queue_depth` proves the ladder at the arithmetic; this
    /// one proves nothing downstream undoes it. The rates are the same pair
    /// that derives 6, so a reader that consulted them at all would be visible.
    #[test]
    fn a_stated_depth_beats_the_derived_default_in_the_table() {
        let inv = queue_image(
            Some(BufferDiscipline::Queue),
            Some(3),
            Some(50.0),
            Some(10.0),
        );
        let row = only_row(&inv);
        assert_eq!(row.depth, 3, "the stated 3, not the derived 6");
        assert_eq!(row.source, DepthSource::Stated);
    }

    /// ACCEPTANCE 3 -- either rate absent means NO default, and the reason is
    /// VISIBLE rather than merely true.
    ///
    /// "Visible" is the bar this campaign sets: a derivation that declines
    /// quietly is indistinguishable from one that never ran, which is how a
    /// green vacuous check survives. So the assertion is on the prose a user
    /// reads, not only on the absence of a row.
    #[test]
    fn a_missing_rate_leaves_the_endpoint_undeclared_with_a_readable_reason() {
        for (publish, drain, want) in [
            (None, Some(10.0), NoDefault::NoPublishRate),
            (Some(50.0), None, NoDefault::NoDrainRate),
        ] {
            let inv = queue_image(Some(BufferDiscipline::Queue), None, publish, drain);
            let DeclaredDepths::Resolved {
                rows,
                undeclared_subscriptions,
                ..
            } = inv.declared_depths()
            else {
                panic!("the table resolves");
            };
            assert!(rows.is_empty(), "no default may be invented: {rows:?}");
            assert_eq!(
                undeclared_subscriptions, 1,
                "the endpoint stays counted as undeclared, so a size consumer still refuses"
            );

            let report = inv.queue_depth_defaults();
            let row = report.first().expect("the endpoint is reported");
            assert_eq!(row.outcome, Err(want.clone()));
            let line = row.line();
            assert!(line.contains("/chatter"), "{line}");
            assert!(
                line.contains("no derived depth"),
                "the line must say a default was not produced: {line}"
            );
            assert!(
                line.contains("rate_hz"),
                "and name the contract key that would produce one: {line}"
            );
        }
    }

    /// ACCEPTANCE 4 -- both diagnostics fire on their shapes, and both are
    /// WARNINGS: `buffer_diagnostics` has no error channel, and neither shape
    /// changes any number the image sizes from.
    #[test]
    fn both_buffer_diagnostics_fire_and_neither_is_an_error() {
        let queue_at_one = queue_image(Some(BufferDiscipline::Queue), Some(1), None, None);
        let d = queue_at_one.buffer_diagnostics();
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message().contains("buffer: queue"), "{:?}", d[0]);
        assert!(d[0].message().contains("Not an error"), "{:?}", d[0]);
        // The stated 1 still sizes the image -- a diagnostic is not a refusal.
        assert_eq!(only_row(&queue_at_one).depth, 1);
        assert_eq!(queue_at_one.declared_depths().tag(), "resolved");

        let latest_at_ten = queue_image(Some(BufferDiscipline::Latest), Some(10), None, None);
        let d = latest_at_ten.buffer_diagnostics();
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message().contains("buffer: latest"), "{:?}", d[0]);
        assert!(d[0].message().contains("Not an error"), "{:?}", d[0]);
        assert_eq!(only_row(&latest_at_ten).depth, 10);
        assert_eq!(latest_at_ten.declared_depths().tag(), "resolved");

        // The agreeing shapes are silent, which is what keeps the warning
        // stream worth reading.
        for (buffer, depth) in [(BufferDiscipline::Queue, 6), (BufferDiscipline::Latest, 1)] {
            assert!(
                queue_image(Some(buffer), Some(depth), None, None)
                    .buffer_diagnostics()
                    .is_empty(),
                "{buffer:?} at depth {depth} is not a contradiction"
            );
        }
    }

    /// A `keep_all` endpoint gets NO derived depth -- the W3 refusal comes
    /// first, for the whole table.
    ///
    /// The one case where this wave could have re-created the defect W3 fixed,
    /// by a different route. W3 refuses a depth STATED beside `keep_all`
    /// because a KEEP_ALL queue has no static bound; a depth this CLI DERIVED
    /// for one would be the same under-size arrived at by arithmetic, and it
    /// would carry more authority, not less.
    #[test]
    fn a_keep_all_queue_endpoint_is_refused_and_never_derived() {
        let inv = queue_image_with_history(
            Some(BufferDiscipline::Queue),
            None,
            Some(50.0),
            Some(10.0),
            QoSHistoryPolicy::KeepAll,
        );
        let d = inv.declared_depths();
        assert_eq!(d.tag(), "refused", "keep_all refuses the whole depth table");
        assert!(d.rows().is_none(), "and publishes no rows at all");
        let DeclaredDepths::Refused { reason } = d else {
            panic!("refused above");
        };
        assert!(
            reason.contains("keep_all") && reason.contains("NO STATIC BOUND"),
            "the W3 reason, not a W8 one: {reason}"
        );
        // The control: the identical image with `keep_last` DOES derive, so the
        // refusal above is the history and not a missing input.
        assert_eq!(
            only_row(&queue_image(
                Some(BufferDiscipline::Queue),
                None,
                Some(50.0),
                Some(10.0)
            ))
            .depth,
            6
        );
    }

    /// ACCEPTANCE 5 -- an image with NO `queue` endpoint is byte-identical.
    ///
    /// The strong form, on the whole fragment rather than on the depth lines:
    /// every `set(` line the image emitted must agree, and the only variables
    /// that may differ are the two W8 adds. This is W3's own proof shape, which
    /// caught the coupling it existed to rule out.
    ///
    /// The CONTROLS matter more than the base case. An image with no rates is
    /// trivially unchanged; the ones that could regress are the image that HAS
    /// both rates and no `buffer:` -- which is every image in this tree today,
    /// since the model drops the key -- and the image that declared
    /// `buffer: latest`. Both must emit the same bytes as the image with no
    /// rates at all, or a default is leaking into endpoints that never asked.
    #[test]
    fn an_image_with_no_queue_endpoint_is_byte_identical() {
        let base = set_lines(&queue_image(None, Some(3), None, None));
        for (label, inv) in [
            (
                "rates, no discipline",
                queue_image(None, Some(3), Some(50.0), Some(10.0)),
            ),
            (
                "buffer: latest with rates",
                queue_image(
                    Some(BufferDiscipline::Latest),
                    Some(3),
                    Some(50.0),
                    Some(10.0),
                ),
            ),
            (
                "no depth and no discipline, rates present",
                queue_image(None, None, Some(50.0), Some(10.0)),
            ),
        ] {
            if label.starts_with("no depth") {
                // This one legitimately differs from `base` (no depth stated),
                // so it is compared against its own pre-W8 shape instead: the
                // endpoint must stay UNDECLARED, with no row and no derived
                // entry.
                let DeclaredDepths::Resolved {
                    rows,
                    undeclared_subscriptions,
                    ..
                } = inv.declared_depths()
                else {
                    panic!("the table resolves");
                };
                assert!(rows.is_empty(), "{label}: {rows:?}");
                assert_eq!(undeclared_subscriptions, 1, "{label}");
                continue;
            }
            assert_eq!(
                set_lines(&inv),
                base,
                "{label}: an image with no `buffer: queue` endpoint must emit the same bytes"
            );
        }

        // And the new variables are PRESENT and EMPTY on such an image rather
        // than absent: an absent list would be indistinguishable from an older
        // CLI's silence, which is exactly why the schema version bumped.
        assert!(
            base.iter()
                .any(|l| l == "set(NROS_ENTITY_DERIVED_DEPTHS \"\")"),
            "{base:?}"
        );
        assert!(
            base.iter()
                .any(|l| l == "set(NROS_ENTITY_DERIVED_DEPTH_COUNT 0)"),
            "{base:?}"
        );
    }

    /// A DERIVED depth sizes the arena and never reaches the compile-time
    /// assertion table.
    ///
    /// The hazard this wave had to avoid, and it is not hypothetical: every row
    /// `to_declared_qos_header` emits becomes a `static_assert` that
    /// `NROS_SUBSCRIBE`'s own QoS must match. A derived default landing there
    /// would turn a DEFAULT into a REQUIREMENT -- an image that stated no depth
    /// would have to spell this CLI's arithmetic at every call site or fail to
    /// compile, and moving `QUEUE_DEPTH_MARGIN` by one slot would break every
    /// such image at once. The ladder inverted.
    #[test]
    fn a_derived_depth_sizes_but_never_asserts() {
        let derived = queue_image(Some(BufferDiscipline::Queue), None, Some(50.0), Some(10.0));
        assert_eq!(only_row(&derived).depth, 6, "it DID size");
        let h = derived.to_declared_qos_header();
        // `#define`, not the bare name: the "no table" branch's own prose says
        // "NROS_DECLARED_QOS_ROWS stays undefined", and matching that sentence
        // would make this assertion pass on a header that DID define the macro.
        assert!(
            !h.contains("#define NROS_DECLARED_QOS_ROWS"),
            "a derived depth must emit NO assertion row: {h}"
        );
        assert!(
            h.contains("stays undefined"),
            "and it takes the no-table branch, which says so: {h}"
        );
        assert!(
            h.contains("NROS_DECLARED_QOS_STATUS \"resolved\""),
            "the table still resolved -- it is the ROWS that are withheld: {h}"
        );
        // The control: the same image with the depth STATED does emit one, so
        // the assertion above cannot pass merely because the header is broken.
        let h = queue_image(Some(BufferDiscipline::Queue), Some(6), None, None)
            .to_declared_qos_header();
        assert!(h.contains("#define NROS_DECLARED_QOS_ROWS"), "{h}");
        assert!(h.contains(", 6)"), "{h}");
    }

    /// The two rates the MODEL does carry reach the endpoint row.
    ///
    /// Not the whole derivation -- `buffer` cannot arrive (issue 1339) -- but
    /// the two halves that can, read by the model's own conventions: the
    /// channel's `contracts.topics.<t>.rate_hz` for the publish rate, and the
    /// `min_rate_hz` of what the node's timer path publishes for the drain
    /// rate. Without this, every `NoDefault` in this wave would read
    /// `NoPublishRate` forever -- a reason that is true and useless.
    #[test]
    fn the_model_supplies_both_rates_a_queue_default_would_divide() {
        let m = model_from_yaml(
            r#"
meta: { version: 1 }
structure:
  nodes:
    /talker:
      { scope: s.launch.xml, pkg: talker_pkg, exec: talker, node_name: talker }
    /listener:
      { scope: s.launch.xml, pkg: listener_pkg, exec: listener,
        node_name: listener }
  topics:
    /chatter:
      type: std_msgs/msg/Int32
      pub: [/talker/chatter]
      sub: [/listener/chatter]
    /status:
      type: std_msgs/msg/Int32
      pub: [/listener/status]
      sub: []
contracts:
  pub_endpoints:
    /listener/status:
      min_rate_hz: 10.0
  sub_endpoints:
    /listener/chatter:
      state: true
  node_paths:
    /listener/drain:
      output: [/listener/status]
  topics:
    /chatter:
      rate_hz: 50.0
"#,
        );
        let inv = EntityInventory::from_model("test", &m).expect("model describes wiring");
        let sub = inv
            .components()
            .iter()
            .flat_map(|c| c.declaration.entities())
            .find(|e| e.kind == EntityKind::Subscription)
            .expect("the listener subscribes")
            .clone();
        assert_eq!(
            sub.publish_rate,
            RateMilliHz::from_hz(50.0),
            "the channel rate is the publish rate"
        );
        assert_eq!(
            sub.drain_rate,
            RateMilliHz::from_hz(10.0),
            "the node's timer path publishes at 10 Hz, so that is its drain rate"
        );
        // And the discipline is the half that did NOT arrive, which is the
        // whole of issue 1339. Asserted here so that the day it does arrive,
        // this test says where to wire it.
        assert_eq!(
            sub.buffer, None,
            "SubContract carries no `buffer` -- see issue 1339"
        );
        // So there is no default, and the REASON names the discipline rather
        // than a rate, because both rates are present.
        let row = inv
            .queue_depth_defaults()
            .into_iter()
            .find(|r| r.topic == "/chatter")
            .expect("the subscription is reported");
        assert_eq!(row.outcome, Err(NoDefault::NotAQueue));
    }
}
