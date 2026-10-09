//! Rust component API shared by metadata discovery and generated runtimes.

// phase-483 W3 — the component API (`Component`, `NodeContext`,
// `DeclarativeNode`) registers into a live executor and exists only with
// `rmw-cffi`. Without it this module still carries the metadata vocabulary the
// C/C++ adapters use, and the declaration helpers the component API calls go
// unused.
#![cfg_attr(not(feature = "rmw-cffi"), allow(unused_imports, dead_code))]

use core::marker::PhantomData;

// issue 0413 — the descriptor-registration bound. `MessageForRmw` is
// `RosMessage` alone unless a descriptor-needing backend is linked, in which
// case it also requires `nros_serdes::schema::Message` (the schema the Cyclone
// descriptor builder walks). Generated message crates implement both.
use nros_node::rmw_type_registry::MessageForRmw;

use crate::{
    ActionTag, CallbackId, CancelResponse, EntityId, GoalId, GoalResponse, GoalStatus,
    ParameterType, QoSProfile, RosAction, RosMessage, RosService, ServiceTag, SubscriptionTag,
    TimerClockSource, TimerDuration,
    heapless::Vec,
    node_metadata::{
        CallbackEffectKind, EntityKind, EntityMetadata, EntityMetadataSpec, MetadataRecorder,
        MetadataString, NodeId, NodeMetadataError, ParameterDefault, SourceLocationMetadata,
        copy_str, entity_metadata,
    },
};

// Phase 212.N.7 step-6 closing sweep — `component_register_symbol`
// removed. It built the legacy `__nros_component_<pkg>_register`
// symbol name for the M.5.a BSP baker to look up by literal. step-6
// retired the macro emit + step-4 deleted the FreeRTOS BSP baker
// crate that was the sole live consumer. The Phase 212.N Entry pkg
// path calls `<pkg>::register(runtime)` through the path API, so this
// helper has no live callers.

/// Result type for component declarations.
pub type NodeResult<T = ()> = Result<T, NodeDeclError>;

/// Register `M`'s runtime type descriptor with a descriptor-needing backend
/// (Cyclone DDS), from the DECLARATIVE path — issue 0413.
///
/// The imperative API's typed creators (`Node::create_publisher_with_qos::<M>`
/// in `nros-node`) already call `register_type::<M>()` before asking the cffi
/// vtable for the entity, because Cyclone resolves topic types through a
/// RUNTIME registry and `dds_create_topic` fails without it.
///
/// The declarative Node API does not reach those creators. `NodeContext`
/// records `EntityMetadata` and the sink calls the type-ERASED
/// `create_generic_publisher_with_qos(topic, type_name, type_hash, qos)` —
/// which has a type NAME and no `M`, so it cannot register anything. The
/// descriptor was therefore never built, `find_descriptor` returned null in
/// `publisher.cpp`, and the entity failed with `NROS_RMW_RET_UNSUPPORTED` ->
/// `TransportError::PublisherCreationFailed` -> `NodeDeclError::Runtime` ->
/// `RuntimeError::NodeRegister("<pkg>")`, four collapses away from the cause.
///
/// So it is registered HERE, at the last point that still knows `M`. A no-op
/// unless a descriptor-needing backend installed a registrar
/// (`nros_rmw::register_type_descriptor` returns `Ok` when the slot is empty),
/// so zenoh / XRCE builds are unaffected.
///
/// Why this only surfaced now: every native Rust example was
/// `[package.metadata.nros.application]` (imperative, typed creators) until
/// phase-338 W3 made them Node-class. C and C++ were never affected — they use
/// the static `descriptors.cpp` table, which is why `c/talker` published
/// normally against the same backend while `rust/talker` could not.
#[inline]
fn register_declared_type<M: nros_node::rmw_type_registry::MessageForRmw>() -> NodeResult<()> {
    nros_node::rmw_type_registry::register_type::<M>().map_err(|_| NodeDeclError::Runtime)
}

/// issue 0413, service half — register both payload types of `S`.
///
/// Services reach the same type-erased sink path as publishers, so the
/// descriptor has to be built here too. The bound is a WHERE-CLAUSE on the
/// declarative methods rather than on `RosService` itself: `RosService` lives in
/// `nros-core`, which cannot depend on `nros-node` where `MessageForRmw` is
/// defined. Generated service crates satisfy it (their payloads implement
/// `schema::Message`); a hand-rolled service used only with zenoh/XRCE is
/// unaffected, because `MessageForRmw` collapses to `RosMessage` when no
/// descriptor-needing backend is linked.
#[inline]
fn register_declared_service<S: RosService>() -> NodeResult<()>
where
    S::Request: nros_node::rmw_type_registry::MessageForRmw,
    S::Reply: nros_node::rmw_type_registry::MessageForRmw,
{
    register_declared_type::<S::Request>()?;
    register_declared_type::<S::Reply>()
}

/// Node declaration error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeDeclError {
    /// Metadata recorder rejected the declaration.
    Metadata(NodeMetadataError),
    /// Host/runtime discovery could not find `nros::node!` export.
    MissingExport,
    /// Generated runtime rejected the declaration.
    Runtime,
    /// The executor's fixed callback-entry table is full — a timer /
    /// subscription / service / action could not claim a slot (issue 0095).
    /// Carries the capacity cause through the `NodeError → NodeDeclError`
    /// collapse so the register seam can name `NROS_EXECUTOR_MAX_CBS`.
    ExecutorFull,
    /// A publish named an entity this component has no publisher for — the
    /// LOOKUP failed, nothing was ever handed to the transport.
    ///
    /// issue 0736 — split out of [`Self::Runtime`], which the publish path
    /// returned for BOTH "the transport rejected the sample" and "there is no
    /// such publisher": `lookup_publisher(...).unwrap_or(Err(Runtime))`. Those
    /// are different bugs in different layers with different fixes, and from a
    /// serial console they were the same line. Exactly the conflation #572
    /// removed one level out, where discarding the result made "the timer never
    /// fired" and "every publish failed" the same observation.
    UnknownPublisher,
    /// The parameter store refused a declaration and the name is still absent:
    /// the store is full, or it rejected the value. A name already declared is
    /// NOT this — the store answers `false` for that too, but the parameter is
    /// there.
    ///
    /// Split out of [`Self::Runtime`] for the reason [`Self::UnknownPublisher`]
    /// was (issue 0736): a refused parameter and a rejected transport handle are
    /// different faults with different fixes, and one opaque variant made them
    /// the same line on a serial console.
    ParameterRejected,
    /// A component CELL registry is full -- the class creates more entities of
    /// one kind (publishers, service servers/clients, action servers/clients)
    /// than its registry holds.
    ///
    /// issue 1130 -- split out of [`Self::Runtime`] BEFORE that capacity became
    /// derivable. phase-412's rule: a derived count is safe only where
    /// exhaustion NAMES the knob, and a bare "component runtime rejected
    /// declaration" named nothing. The capacity is the class's `ENTITY_BOUNDS`
    /// when it states one, else `NROS_RUNTIME_MAX_CELL_ENTITIES`.
    CellRegistryFull,
}

impl NodeDeclError {
    /// Human-readable static message for diagnostics that cross FFI/plugin boundaries.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Metadata(NodeMetadataError::Capacity) => "component metadata capacity exceeded",
            Self::Metadata(NodeMetadataError::NameTooLong) => "component metadata name too long",
            Self::Metadata(NodeMetadataError::UnknownNode) => {
                "component entity references an unknown node"
            }
            Self::Metadata(NodeMetadataError::UnknownEntity) => {
                "component callback effect references an unknown entity"
            }
            Self::Metadata(NodeMetadataError::DuplicateId) => {
                "component metadata contains a duplicate stable ID"
            }
            Self::MissingExport => "package has no exported nros component",
            Self::Runtime => "component runtime rejected declaration",
            Self::UnknownPublisher => "no publisher declared for that entity",
            Self::ParameterRejected => {
                "the parameter store refused the declaration (the store is full, or it rejected the value)"
            }
            Self::ExecutorFull => {
                "executor callback table full — raise NROS_EXECUTOR_MAX_CBS \
                 (build-time, default 4)"
            }
            Self::CellRegistryFull => {
                "component cell registry full — raise the class's ENTITY_BOUNDS, \
                 or NROS_RUNTIME_MAX_CELL_ENTITIES (build-time, default 8) for a \
                 class that states none"
            }
        }
    }
}

impl From<NodeMetadataError> for NodeDeclError {
    fn from(value: NodeMetadataError) -> Self {
        Self::Metadata(value)
    }
}

#[cfg(test)]
mod cell_registry_full_tests {
    use super::NodeDeclError;

    /// Issue 1130 -- the precondition for deriving the cell capacity: running
    /// out NAMES both remedies, where it used to read "component runtime
    /// rejected declaration".
    #[test]
    fn a_full_cell_registry_names_the_knob_and_the_class_bound() {
        let m = NodeDeclError::CellRegistryFull.message();
        assert!(m.contains("NROS_RUNTIME_MAX_CELL_ENTITIES"), "{m}");
        assert!(m.contains("ENTITY_BOUNDS"), "{m}");
        assert_ne!(m, NodeDeclError::Runtime.message());
    }
}

/// issue 0413, action half — register every wire payload type of `A`.
///
/// Mirrors the eight `register_type::<A::…>()` calls the IMPERATIVE action
/// creator makes (`nros-node/src/executor/action.rs`); the declarative path
/// reaches the same type-erased sink and would otherwise register none of them.
/// `A::register_protocol_types()` covers the fixed `action_msgs` types the
/// cancel/status plumbing serializes, exactly as the imperative path does.
#[inline]
fn register_declared_action<A: RosAction>() -> NodeResult<()>
where
    A::Goal: nros_node::rmw_type_registry::MessageForRmw,
    A::Result: nros_node::rmw_type_registry::MessageForRmw,
    A::Feedback: nros_node::rmw_type_registry::MessageForRmw,
    A::SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
    A::SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
    A::GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
    A::GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
    A::FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
{
    register_declared_type::<A::Goal>()?;
    register_declared_type::<A::Result>()?;
    register_declared_type::<A::Feedback>()?;
    register_declared_type::<A::SendGoalRequest>()?;
    register_declared_type::<A::SendGoalResponse>()?;
    register_declared_type::<A::GetResultRequest>()?;
    register_declared_type::<A::GetResultResponse>()?;
    register_declared_type::<A::FeedbackMessage>()?;
    A::register_protocol_types().map_err(|()| NodeDeclError::Runtime)
}

/// phase-391 W5-endgame step 2c (issue 0857) — a component class's declared
/// upper bounds, PER ENTITY KIND, for sizing its cell registries at compile
/// time.
///
/// The runtime's per-class cell storage is static, so its registries pay
/// their CAPACITY whether or not entities fill it — and one publisher slot
/// costs ~1.35 KiB (the loan arena rides inside). The default is the
/// `NROS_RUNTIME_MAX_CELL_ENTITIES` knob per kind, which always works;
/// a class that declares its real bounds pays exactly what it uses.
/// Declaring FEWER than `register()` creates is a loud registration error
/// (registry full), never a silent drop.
///
/// Public and non-generic on purpose (the `ExecutorSizing` rule): the const
/// generics this feeds stay behind the `nros::node!` macro emission, so no
/// other language ever sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityBounds {
    /// Publishers this class creates (each slot ~1.35 KiB — the big one).
    pub publishers: usize,
    /// Service servers (sizes the trampoline-context slab, not a registry).
    pub service_servers: usize,
    /// Service clients.
    pub service_clients: usize,
    /// Action clients.
    pub action_clients: usize,
    /// Action servers.
    pub action_servers: usize,
}

impl EntityBounds {
    /// The knob-capped default (`NROS_RUNTIME_MAX_CELL_ENTITIES` per kind).
    pub const fn knob_caps() -> Self {
        Self {
            publishers: crate::config::MAX_CELL_ENTITIES,
            service_servers: crate::config::MAX_CELL_ENTITIES,
            service_clients: crate::config::MAX_CELL_ENTITIES,
            action_clients: crate::config::MAX_CELL_ENTITIES,
            action_servers: crate::config::MAX_CELL_ENTITIES,
        }
    }

    /// Exact bounds, spelled positionally:
    /// `(publishers, service_servers, service_clients, action_clients, action_servers)`.
    pub const fn exact(
        publishers: usize,
        service_servers: usize,
        service_clients: usize,
        action_clients: usize,
        action_servers: usize,
    ) -> Self {
        Self {
            publishers,
            service_servers,
            service_clients,
            action_clients,
            action_servers,
        }
    }
}

/// Rust component entry point.
#[cfg(feature = "rmw-cffi")]
pub trait Component {
    /// Source component name used in metadata and diagnostics.
    const NAME: &'static str;

    /// Phase 216.A.3 — declares which dispatch strategy this Node
    /// requires from the runtime. Defaults to
    /// [`crate::DispatchStrategy::Inline`] so every existing component
    /// keeps compiling without source change; the substrate (Phase
    /// 216.A.2) and `nros check` (Phase 216.D.1) consume it to
    /// pick / validate the board-side dispatch path.
    const DISPATCH: crate::DispatchStrategy = crate::DispatchStrategy::Inline;

    /// phase-391 W5-endgame step 2c — this class's per-kind entity bounds,
    /// sizing its static cell registries. Defaults to the knob caps so every
    /// existing component keeps compiling; declare [`EntityBounds::exact`]
    /// to stop paying for capacity `register()` never fills.
    const ENTITY_BOUNDS: EntityBounds = EntityBounds::knob_caps();

    /// Declare nodes, entities, callbacks, params, and optional effects.
    fn register(context: &mut NodeContext<'_>) -> NodeResult<()>;
}

/// Runtime-neutral node construction options.
///
/// ADOPT-BOUNDED against rclrs's `NodeOptions` / `IntoNodeOptions` builder: three
/// public fields (`name`, `namespace`, `domain_id`) and nothing else. There are no
/// node ARGUMENTS and no rosout toggle — both are fixed at build time here
/// (RFC-0045), so an rclrs builder call for either is a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeOptions<'a> {
    /// Source node name. Launch planning may remap/namespace later.
    pub name: &'a str,
    /// Source namespace. Defaults to `/`.
    pub namespace: &'a str,
    /// ROS domain ID hint. Defaults to `0`.
    pub domain_id: u32,
}

/// Runtime callback event delivered to an executable Node.
///
/// The value carries the source callback name declared by the component, but
/// does not expose the generated/internal callback ID type to product code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Callback<'a> {
    id: CallbackId<'a>,
}

impl<'a> Callback<'a> {
    /// Borrow the source callback name.
    pub const fn as_str(self) -> &'a str {
        self.id.as_str()
    }

    /// Return true when this callback matches `name`.
    pub fn is_named(self, name: &str) -> bool {
        self.as_str() == name
    }

    /// Build a callback event from the internal/generated callback ID.
    #[doc(hidden)]
    pub const fn __from_id(id: CallbackId<'a>) -> Self {
        Self { id }
    }
}

impl<'a> NodeOptions<'a> {
    /// Create node options with default namespace and domain.
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            namespace: "/",
            domain_id: 0,
        }
    }

    /// Set source namespace.
    pub const fn namespace(mut self, namespace: &'a str) -> Self {
        self.namespace = namespace;
        self
    }

    /// Set ROS domain ID hint.
    pub const fn domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}

/// Declaration sink implemented by metadata recorders and generated runtimes.
pub trait NodeRuntime {
    /// Declare a component node.
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> NodeResult<()>;

    /// Declare a publisher, subscription, timer, service, action, or parameter.
    fn create_entity(&mut self, metadata: EntityMetadata) -> NodeResult<()>;

    /// Add optional callback effect metadata.
    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> NodeResult<()>;
}

impl<const MAX_NODES: usize, const MAX_ENTITIES: usize, const MAX_CALLBACKS: usize> NodeRuntime
    for MetadataRecorder<MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> NodeResult<()> {
        self.push_node(id, options.name, options.namespace, options.domain_id)?;
        Ok(())
    }

    fn create_entity(&mut self, mut metadata: EntityMetadata) -> NodeResult<()> {
        // phase-457 W5 (issue 1522) — state this subscription's registration
        // path, because on THIS road nothing will observe it.
        //
        // `Executor::open_subscription` reports `in_place_capable` to
        // `registration_observer`, which is how the C/C++ probes get theirs.
        // The Rust probe (`record_node_metadata::<C>`) runs `register()`
        // against this recorder and opens no executor, so that seam never
        // fires and every Rust endpoint's row refused — leaving every consumer
        // on the receive-region budget for a fact the declaration determines.
        //
        // This is the ONE `NodeRuntime` seam a Rust declaration crosses; the
        // C/C++ adapters reach the recorder through `push_entity` and are
        // untouched, so their observed fact still wins its own way.
        //
        // Not a second opinion: `declared_subscription_shape` is the same call
        // the declarative registrar branches on, and its
        // `in_place_capable()` is the same expression the entry point writes
        // into its `SubscriptionRequest`. Gated by
        // `check-declared-subscription-shape`.
        if metadata.kind == EntityKind::Subscription && metadata.in_place_capable.is_none() {
            metadata.in_place_capable =
                Some(metadata.declared_subscription_shape().in_place_capable());
        }
        self.push_entity(metadata)?;
        Ok(())
    }

    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> NodeResult<()> {
        self.push_callback_effect(callback_id, kind, entity_id)?;
        Ok(())
    }
}

/// phase-483 W3 — where a component's declarations go while its `register`
/// runs: the live component runtime (`ExecutorSink`, which owns the
/// component's cell) or the metadata recorder (the host probe). Both receive
/// the executor explicitly, because the [`crate::Node`] a component holds
/// borrows that same executor; a sink that held it too would alias it.
#[cfg(feature = "rmw-cffi")]
pub(crate) trait FrameSink {
    /// Create the executor node a component asked for and return its id.
    fn create_node(
        &mut self,
        executor: &mut crate::Executor<'static>,
        id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> NodeResult<nros_node::executor::NodeId>;

    /// Declare one entity.
    fn create_entity(
        &mut self,
        executor: &mut crate::Executor<'static>,
        metadata: EntityMetadata,
    ) -> NodeResult<()>;

    /// Record one callback effect.
    fn record_callback_effect(
        &mut self,
        executor: &mut crate::Executor<'static>,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> NodeResult<()>;
}

/// The most nodes one component registration may create. A component
/// declares one in every shipped shape.
#[cfg(feature = "rmw-cffi")]
const MAX_FRAME_NODES: usize = 4;

#[cfg(feature = "rmw-cffi")]
struct FrameNode {
    exec_id: nros_node::executor::NodeId,
    stable: MetadataString,
    group: Option<MetadataString>,
}

/// phase-483 W3 — one component registration in progress. Installed on the
/// executor (`Executor::__set_component_frame`) for exactly the lifetime of
/// the [`NodeContext`] that owns it, which is how a [`DeclarativeNode`]
/// method on a plain [`crate::Node`] finds the sink.
#[cfg(feature = "rmw-cffi")]
pub(crate) struct ComponentFrame<'a> {
    pub(crate) sink: &'a mut dyn FrameSink,
    nodes: Vec<FrameNode, MAX_FRAME_NODES>,
}

#[cfg(feature = "rmw-cffi")]
impl<'a> ComponentFrame<'a> {
    pub(crate) fn new(sink: &'a mut dyn FrameSink) -> Self {
        Self {
            sink,
            nodes: Vec::new(),
        }
    }

    fn push(&mut self, exec_id: nros_node::executor::NodeId, stable: &str) -> NodeResult<()> {
        self.nodes
            .push(FrameNode {
                exec_id,
                stable: copy_str(stable)?,
                group: None,
            })
            .map_err(|_| NodeDeclError::Metadata(NodeMetadataError::Capacity))
    }

    fn node(&self, exec_id: nros_node::executor::NodeId) -> NodeResult<&FrameNode> {
        self.nodes
            .iter()
            .find(|n| n.exec_id == exec_id)
            .ok_or(NodeDeclError::Metadata(NodeMetadataError::UnknownNode))
    }

    fn stable_id_of(&self, exec_id: nros_node::executor::NodeId) -> NodeResult<NodeId<'_>> {
        Ok(NodeId::new(self.node(exec_id)?.stable.as_str()))
    }

    fn group_of(&self, exec_id: nros_node::executor::NodeId) -> Option<MetadataString> {
        self.node(exec_id).ok().and_then(|n| n.group.clone())
    }

    fn set_group(&mut self, stable: &str, group: Option<MetadataString>) -> NodeResult<()> {
        let node = self
            .nodes
            .iter_mut()
            .find(|n| n.stable.as_str() == stable)
            .ok_or(NodeDeclError::Metadata(NodeMetadataError::UnknownNode))?;
        node.group = group;
        Ok(())
    }
}

/// The registration frame installed on `executor`, or `Runtime` when no
/// component registration is in progress.
///
/// The `'static` is a lie told to the borrow checker and kept honest by
/// [`NodeContext`]: the frame is installed when the context is built and
/// cleared when it drops, and every caller here runs inside that window and
/// drops the reference before returning.
#[cfg(feature = "rmw-cffi")]
fn frame_mut(
    executor: &crate::Executor<'static>,
) -> NodeResult<&'static mut ComponentFrame<'static>> {
    let ptr = executor.__component_frame().ok_or(NodeDeclError::Runtime)?;
    // SAFETY: installed by `NodeContext::new` from a `&mut ComponentFrame`
    // that outlives the context, and cleared by its `Drop`; the frame is
    // separate memory from the executor, so this does not alias the
    // executor borrow the caller also holds.
    Ok(unsafe { &mut *ptr.cast::<ComponentFrame<'static>>().as_ptr() })
}

/// The metadata recorder as a registration sink — the host probe's road.
///
/// It records every declaration and ALSO creates the node on the executor
/// the probe opened on the `metadata` backend, so the component's `register`
/// gets a real [`crate::Node`] exactly as it does on a board.
#[cfg(feature = "rmw-cffi")]
pub(crate) struct RecordSink<'r> {
    pub(crate) recorder: &'r mut dyn NodeRuntime,
}

#[cfg(feature = "rmw-cffi")]
impl FrameSink for RecordSink<'_> {
    fn create_node(
        &mut self,
        executor: &mut crate::Executor<'static>,
        id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> NodeResult<nros_node::executor::NodeId> {
        self.recorder.create_node(id, options)?;
        executor
            .node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| NodeDeclError::Runtime)
    }

    fn create_entity(
        &mut self,
        _executor: &mut crate::Executor<'static>,
        metadata: EntityMetadata,
    ) -> NodeResult<()> {
        self.recorder.create_entity(metadata)
    }

    fn record_callback_effect(
        &mut self,
        _executor: &mut crate::Executor<'static>,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> NodeResult<()> {
        self.recorder
            .record_callback_effect(callback_id, kind, entity_id)
    }
}

/// Node declaration context — what a component's
/// [`register`](Component::register) receives.
///
/// phase-483 W3: it creates REAL nodes. [`create_node`](Self::create_node)
/// returns a [`crate::Node`], the same type `Executor::create_node` gives a
/// standalone program, so a component's node code and a program's node code
/// are one code. The component-only declarations are the
/// [`DeclarativeNode`] methods on that node.
#[cfg(feature = "rmw-cffi")]
pub struct NodeContext<'a> {
    component_name: &'static str,
    executor: &'a mut crate::Executor<'static>,
    /// Phase 264 W4a — this node instance's parameters, the COMPILE-BAKED initial
    /// values from the launch `<param name=… value=…/>` entries (`nros::main!`
    /// bakes them + threads them through `install_node_typed_with_params`). Empty
    /// for the metadata-recorder / no-launch paths. A node reads them in
    /// `register()` via [`param`](Self::param) and stashes the typed value on its
    /// `State` (RFC-0004 §10 — baked initials; runtime reconfig is W4b).
    params: &'a [(&'a str, &'a str)],
}

#[cfg(feature = "rmw-cffi")]
impl<'a> NodeContext<'a> {
    /// Build a context over `executor`, installing `frame` on it until this
    /// context drops.
    pub(crate) fn new(
        component_name: &'static str,
        executor: &'a mut crate::Executor<'static>,
        frame: &'a mut ComponentFrame<'_>,
    ) -> Self {
        executor.__set_component_frame(Some(core::ptr::NonNull::from(frame).cast()));
        Self {
            component_name,
            executor,
            params: &[],
        }
    }

    /// Phase 264 W4a — seed this node instance's baked launch parameters (called by
    /// `install_node_typed_with_params` before `Component::register`).
    pub fn set_params(&mut self, params: &'a [(&'a str, &'a str)]) {
        self.params = params;
    }

    /// Phase 264 W4a — the baked initial value of launch parameter `name`, or
    /// `None` if the launch declared no `<param name="…"/>` for this node
    /// instance. Read in `register()` and parse/stash on `State` (RFC-0004 §10).
    pub fn param(&self, name: &str) -> Option<&'a str> {
        self.params
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
    }

    /// Source component name.
    pub const fn component_name(&self) -> &'static str {
        self.component_name
    }

    /// Create a node with an explicit stable node ID.
    ///
    /// Generated/internal form; product code should use
    /// [`create_node`](Self::create_node).
    #[doc(hidden)]
    pub fn create_node_with_id(
        &mut self,
        id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> NodeResult<crate::Node<'_, 'static>> {
        let frame = frame_mut(self.executor)?;
        let exec_id = frame.sink.create_node(self.executor, id, options)?;
        frame.push(exec_id, id.as_str())?;
        Ok(self.executor.node_mut(exec_id))
    }

    /// Create this component's node — rclrs's `executor.create_node(options)`.
    ///
    /// The node's name is the launch file's when it names one, otherwise
    /// `options.name`, which is also the component-stable node id the
    /// declarations are keyed on.
    pub fn create_node(
        &mut self,
        options: NodeOptions<'_>,
    ) -> NodeResult<crate::Node<'_, 'static>> {
        self.create_node_with_id(NodeId::new(options.name), options)
    }

    /// Record optional effects for a callback not tied to a node.
    #[doc(hidden)]
    pub fn callback<'id>(&mut self, id: CallbackId<'id>) -> CallbackEffects<'_, 'id> {
        CallbackEffects {
            executor: self.executor,
            id,
        }
    }
}

#[cfg(feature = "rmw-cffi")]
impl Drop for NodeContext<'_> {
    fn drop(&mut self) {
        self.executor.__set_component_frame(None);
    }
}

/// phase-483 W3 — the DECLARATIVE entity constructors, on the one node type.
///
/// A component's `register` creates its node with
/// [`NodeContext::create_node`] and gets a [`crate::Node`], the same type a
/// standalone program gets from `Executor::create_node`. Everything rclrs's
/// node has is an inherent method there. The constructors here are the RTOS
/// extension a component adds on top: each one DECLARES an entity — a stable
/// id, a callback NAME, effects — to the component runtime, which owns the
/// entity in the component's static cell and dispatches its callback by name
/// to [`ExecutableNode::on_callback`]. That is what framework dispatch
/// (RTIC/Embassy) and the metadata probe need, and what a closure cannot give
/// them.
///
/// Only callable inside a component's `register`: outside one there is no
/// component to declare into, and every method answers
/// [`NodeDeclError::Runtime`].
///
/// The explicit-id forms that would collide with an rclrs-named inherent
/// method on `Node` are spelled `declare_*` (`declare_publisher`,
/// `declare_subscription`, `declare_timer`, …).
#[cfg(feature = "rmw-cffi")]
pub trait DeclarativeNode {
    /// The component-stable id of this node. Not a user API.
    #[doc(hidden)]
    fn __node_id(&mut self) -> NodeResult<NodeId<'static>>;

    /// Hand one entity declaration to the component runtime. Not a user API.
    #[doc(hidden)]
    fn __declare_entity(&mut self, metadata: EntityMetadata) -> NodeResult<()>;

    /// The executor the node borrows. Not a user API.
    #[doc(hidden)]
    fn __executor_mut(&mut self) -> &mut crate::Executor<'static>;

    /// Set the sticky callback-group label applied to every entity
    /// declared after this call (until changed again). The group is the
    /// symbolic name the node author exposes; `system.toml` maps it to a
    /// scheduling tier (RFC-0015). Entities declared while no group is set
    /// remain unlabeled (wildcard-eligible).
    #[track_caller]
    fn callback_group(&mut self, group: &str) -> NodeResult<&mut Self>
    where
        Self: Sized,
    {
        let group = copy_str(group)?;
        let executor = self.__executor_mut() as *mut crate::Executor<'static>;
        // SAFETY: `executor` is the node's own borrow, live for this call.
        let frame = frame_mut(unsafe { &*executor })?;
        let node = self.__node_id()?;
        frame.set_group(node.as_str(), Some(group))?;
        Ok(self)
    }

    /// Declare a publisher with default QoS. Stable publisher ID is required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_publisher<'entity, M: MessageForRmw>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.declare_publisher_with_qos::<M>(id, topic, QoSProfile::default())
    }

    /// Declare a publisher using `topic` as the stable entity ID.
    ///
    /// Use the explicit [`declare_publisher`](Self::declare_publisher) form when
    /// a node declares more than one publisher on the same topic or needs a
    /// stable metadata ID that differs from the ROS topic name.
    #[track_caller]
    fn create_publisher_for_topic<'entity, M: MessageForRmw>(
        &mut self,
        topic: &'entity str,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.create_publisher_for_topic_with_qos::<M>(topic, QoSProfile::default())
    }

    /// Declare a publisher with explicit QoS, using `topic` as the stable entity ID.
    #[track_caller]
    fn create_publisher_for_topic_with_qos<'entity, M: MessageForRmw>(
        &mut self,
        topic: &'entity str,
        qos: QoSProfile,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.declare_publisher_with_qos::<M>(EntityId::new(topic), topic, qos)
    }

    /// Declare a publisher with explicit QoS.
    #[track_caller]
    #[doc(hidden)]
    fn declare_publisher_with_qos<'entity, M: MessageForRmw>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
        qos: QoSProfile,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        register_declared_type::<M>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Publisher,
            source_name: topic,
            // issue 0413 — `MessageForRmw` can pull `schema::Message` into scope,
            // which also has a `TYPE_NAME`; disambiguate to the ROS-facing one.
            type_name: <M as RosMessage>::TYPE_NAME,
            type_hash: <M as RosMessage>::TYPE_HASH,
            qos,
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.__declare_entity(metadata)?;
        Ok(NodePublisher::new(id))
    }

    /// Declare a subscription. Stable subscription and callback IDs are required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_subscription<'entity, 'callback, M: MessageForRmw>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.declare_subscription_with_qos::<M>(id, callback_id, topic, QoSProfile::default())
    }

    /// Declare a subscription using `callback_id` as the stable entity ID.
    ///
    /// Generated/internal form; product code should use
    /// [`create_subscription_for_callback_name`](Self::create_subscription_for_callback_name).
    #[track_caller]
    #[doc(hidden)]
    fn create_subscription_for_callback<'callback, M: MessageForRmw>(
        &mut self,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        self.create_subscription_for_callback_with_qos::<M>(
            callback_id,
            topic,
            QoSProfile::default(),
        )
    }

    /// Declare a subscription using `callback_name` as the source callback
    /// name and synthesized entity ID.
    #[track_caller]
    fn create_subscription_for_callback_name<'callback, M: MessageForRmw>(
        &mut self,
        callback_name: &'callback str,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        self.create_subscription_for_callback::<M>(CallbackId::new(callback_name), topic)
    }

    /// Phase 250 (Wave 2b) — declare a subscription with E2E message-integrity
    /// validation enabled (the declarative `.safety()` opt-in). Identical to
    /// [`create_subscription_for_callback_name`](Self::create_subscription_for_callback_name)
    /// but flags the entity so the runtime registers it via
    /// `create_generic_subscription_with_integrity`; the callback then reads
    /// [`CallbackCtx::integrity`](CallbackCtx::integrity) alongside the message.
    /// The config-driven `[safety]` axis (Wave 4 codegen) emits this call; it is
    /// also usable by hand. Ungated — when `safety-e2e` is off the flag is simply
    /// ignored and the subscription registers as a basic one.
    #[track_caller]
    fn create_subscription_for_callback_name_with_safety<'callback, M: MessageForRmw>(
        &mut self,
        callback_name: &'callback str,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        let callback_id = CallbackId::new(callback_name);
        let id = EntityId::new(callback_id.as_str());
        register_declared_type::<M>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Subscription,
            source_name: topic,
            // issue 0413 — `MessageForRmw` can pull `schema::Message` into scope,
            // which also has a `TYPE_NAME`; disambiguate to the ROS-facing one.
            type_name: <M as RosMessage>::TYPE_NAME,
            type_hash: <M as RosMessage>::TYPE_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        metadata.safety = true;
        self.__declare_entity(metadata)?;
        Ok(NodeSubscription::new(id))
    }

    /// Declare a subscription with explicit QoS, using `callback_id` as the stable entity ID.
    #[track_caller]
    #[doc(hidden)]
    fn create_subscription_for_callback_with_qos<'callback, M: MessageForRmw>(
        &mut self,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QoSProfile,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        self.declare_subscription_with_qos::<M>(
            EntityId::new(callback_id.as_str()),
            callback_id,
            topic,
            qos,
        )
    }

    /// Declare a subscription using `topic` as both the stable entity ID and callback ID.
    #[track_caller]
    fn create_subscription_for_topic<'entity, M: MessageForRmw>(
        &mut self,
        topic: &'entity str,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.create_subscription_for_topic_with_qos::<M>(topic, QoSProfile::default())
    }

    /// Declare a subscription with explicit QoS, using `topic` as both IDs.
    #[track_caller]
    fn create_subscription_for_topic_with_qos<'entity, M: MessageForRmw>(
        &mut self,
        topic: &'entity str,
        qos: QoSProfile,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.declare_subscription_with_qos::<M>(
            EntityId::new(topic),
            CallbackId::new(topic),
            topic,
            qos,
        )
    }

    /// Declare a subscription with explicit QoS.
    #[track_caller]
    #[doc(hidden)]
    fn declare_subscription_with_qos<'entity, 'callback, M: MessageForRmw>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QoSProfile,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        // Phase 380 W4 — a subscription whose receive buffer provably cannot
        // hold its own message type fails the BUILD, not the field.
        //
        // Without this the sample is received, ACKed, and then dropped, and
        // `report_dropped_take` can only say "raise the knob" because nothing
        // knows what value would have worked (issues 0757, 0776). The number is
        // known at compile time for any BOUNDED type, so the check costs
        // nothing at runtime and cannot be forgotten at a call site.
        //
        // `bound_fits`, not `buffer_fits`: an UNBOUNDED type passes, because
        // there is nothing to prove and no finite buffer fits a `String` — the
        // other predicate would refuse the most common message in ROS. Both
        // encodings are checked and the larger taken; the peer chooses the
        // encoding at runtime, so sizing from XCDR1 alone is a trap.
        // Only where `MessageForRmw` guarantees a schema. The other arm accepts a
        // hand-written `RosMessage` with none (see `rmw_type_registry`), and
        // requiring one there would make codegen mandatory for a user's own
        // message type — too large a price for a build assertion. Backends that
        // register type descriptors already demand the schema, so this is free
        // there and absent elsewhere; `size::bound_fits` stays public so a
        // caller can assert it explicitly.
        const {
            assert!(
                nros_node::rmw_type_registry::subscription_buffer_ok::<M>(),
                "this message type's maximum serialized size exceeds \
                 NROS_SUBSCRIPTION_BUFFER_SIZE — every sample would be received, \
                 ACKed and then DROPPED. Raise the knob to at least the type's \
                 bound (`<M as nros_serdes::schema::Message>::MAX_SERIALIZED_SIZE_XCDR2`)."
            )
        }
        register_declared_type::<M>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Subscription,
            source_name: topic,
            // issue 0413 — `MessageForRmw` can pull `schema::Message` into scope,
            // which also has a `TYPE_NAME`; disambiguate to the ROS-facing one.
            type_name: <M as RosMessage>::TYPE_NAME,
            type_hash: <M as RosMessage>::TYPE_HASH,
            qos,
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.__declare_entity(metadata)?;
        Ok(NodeSubscription::new(id))
    }

    /// Declare a subscription whose stable entity and callback IDs are
    /// both synthesized from the topic literal, returning a
    /// [`SubscriptionTag`] the Node author stores on `Self::State` and
    /// matches against the `Callback<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// Use this on the Phase 216.A Deferred Node path where the Node
    /// author does not need to invent a separate stable entity ID — the
    /// topic literal becomes both the entity ID and the callback ID,
    /// and the returned tag preserves that identifier for compile-time
    /// `state.sub_chatter == cb` matches in `on_callback`.
    #[track_caller]
    fn create_subscription_static<M: MessageForRmw>(
        &mut self,
        topic: &'static str,
    ) -> NodeResult<SubscriptionTag> {
        let id = EntityId::new(topic);
        let callback_id = CallbackId::new(topic);
        register_declared_type::<M>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Subscription,
            source_name: topic,
            // issue 0413 — `MessageForRmw` can pull `schema::Message` into scope,
            // which also has a `TYPE_NAME`; disambiguate to the ROS-facing one.
            type_name: <M as RosMessage>::TYPE_NAME,
            type_hash: <M as RosMessage>::TYPE_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.__declare_entity(metadata)?;
        Ok(SubscriptionTag::new(topic))
    }

    /// Declare a timer. Stable timer and callback IDs are required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_timer<'entity, 'callback>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> NodeResult<NodeTimer<'entity>> {
        // phase-430 W4 — the wall spelling IS the `Steady` clock, so it is the
        // same declaration with the axis defaulted rather than a second body.
        // `#[track_caller]` on both hops keeps `SourceLocationMetadata::caller`
        // pointing at the component, not at this line.
        self.declare_timer_on_clock(id, callback_id, period, TimerClockSource::Steady)
    }

    /// Declare a timer using `callback_id` as the stable timer entity ID.
    #[track_caller]
    #[doc(hidden)]
    fn create_timer_for_callback<'callback>(
        &mut self,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> NodeResult<NodeTimer<'callback>> {
        self.declare_timer(EntityId::new(callback_id.as_str()), callback_id, period)
    }

    /// Declare a timer using `callback_name` as the source callback name and
    /// synthesized entity ID.
    #[track_caller]
    fn create_timer_for_callback_name<'callback>(
        &mut self,
        callback_name: &'callback str,
        period: TimerDuration,
    ) -> NodeResult<NodeTimer<'callback>> {
        self.create_timer_for_callback(CallbackId::new(callback_name), period)
    }

    /// Declare a timer on a chosen CLOCK — phase-430 W4, the declarative
    /// spelling of [`Executor::register_timer_on_clock`](crate::Executor::register_timer_on_clock)
    /// and the counterpart of C++'s `create_timer(clock, period, cb)`.
    ///
    /// [`create_timer`](Self::create_timer) is this with
    /// [`TimerClockSource::Steady`], which is the WALL case and is what every
    /// timer declared before W4 gets: the runtime registers it exactly as it
    /// always did. A [`TimerClockSource::Ros`] timer instead follows `/clock` —
    /// it stops while the simulator is paused and tracks a bag's replay rate —
    /// and with no `/clock` source installed it reads system time, the same
    /// fallback `rclcpp::Clock` has.
    #[track_caller]
    #[doc(hidden)]
    fn declare_timer_on_clock<'entity, 'callback>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
        clock: TimerClockSource,
    ) -> NodeResult<NodeTimer<'entity>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Timer,
            source_name: "",
            type_name: "",
            type_hash: "",
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        metadata.period_ms = Some(period.as_millis());
        metadata.period_us = Some(period.as_micros());
        metadata.timer_clock = clock;
        self.__declare_entity(metadata)?;
        Ok(NodeTimer::new(id))
    }

    /// [`declare_timer_on_clock`](Self::declare_timer_on_clock) using
    /// `callback_id` as the stable timer entity ID.
    #[track_caller]
    #[doc(hidden)]
    fn create_timer_for_callback_on_clock<'callback>(
        &mut self,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
        clock: TimerClockSource,
    ) -> NodeResult<NodeTimer<'callback>> {
        self.declare_timer_on_clock(
            EntityId::new(callback_id.as_str()),
            callback_id,
            period,
            clock,
        )
    }

    /// [`declare_timer_on_clock`](Self::declare_timer_on_clock) using
    /// `callback_name` as the source callback name and synthesized entity ID —
    /// the spelling a `nros::main!` component writes:
    ///
    /// ```ignore
    /// node.create_timer_for_callback_name_on_clock(
    ///     "on_tick",
    ///     TimerDuration::from_millis(100),
    ///     TimerClockSource::Ros,
    /// )?;
    /// ```
    #[track_caller]
    fn create_timer_for_callback_name_on_clock<'callback>(
        &mut self,
        callback_name: &'callback str,
        period: TimerDuration,
        clock: TimerClockSource,
    ) -> NodeResult<NodeTimer<'callback>> {
        self.create_timer_for_callback_on_clock(CallbackId::new(callback_name), period, clock)
    }

    /// Declare a service server. Stable service and callback IDs are required.
    #[track_caller]
    #[doc(hidden)]
    fn create_service_server<
        'entity,
        'callback,
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        service_name: &str,
    ) -> NodeResult<NodeServiceServer<'entity, S>> {
        register_declared_service::<S>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::ServiceServer,
            source_name: service_name,
            type_name: S::SERVICE_NAME,
            type_hash: S::SERVICE_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.__declare_entity(metadata)?;
        Ok(NodeServiceServer::new(id))
    }

    /// Declare a service server using `name` as both the stable entity ID
    /// and callback ID.
    #[track_caller]
    fn create_service_server_for_name<
        'entity,
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
    ) -> NodeResult<NodeServiceServer<'entity, S>> {
        self.create_service_server::<S>(EntityId::new(name), CallbackId::new(name), name)
    }

    /// Declare a service server using `name` as the stable entity ID and
    /// `callback_name` as the source callback name.
    #[track_caller]
    fn create_service_server_for_name_with_callback<
        'entity,
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
        callback_name: &str,
    ) -> NodeResult<NodeServiceServer<'entity, S>> {
        self.create_service_server::<S>(EntityId::new(name), CallbackId::new(callback_name), name)
    }

    /// Declare a service server whose stable entity and callback IDs are
    /// both synthesized from the service-name literal, returning a
    /// [`ServiceTag`] the Node author stores on `Self::State` and matches
    /// against the `Callback<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// Tag-only registration is restricted to the SERVER side: clients
    /// need a USABLE handle (`NodeServiceClient`) to issue requests, so
    /// use the existing
    /// [`create_service_client_for_name`](Self::create_service_client_for_name) builder
    /// for the client side.
    #[track_caller]
    fn create_service_static<
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'static str,
    ) -> NodeResult<ServiceTag> {
        self.create_service_server_for_name::<S>(name)?;
        Ok(ServiceTag::new(name))
    }

    /// Declare a service client. Stable service client ID is required.
    #[track_caller]
    #[doc(hidden)]
    fn create_service_client<
        'entity,
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        id: EntityId<'entity>,
        service_name: &str,
    ) -> NodeResult<NodeServiceClient<'entity, S>> {
        register_declared_service::<S>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::ServiceClient,
            source_name: service_name,
            type_name: S::SERVICE_NAME,
            type_hash: S::SERVICE_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.__declare_entity(metadata)?;
        Ok(NodeServiceClient::new(id))
    }

    /// Declare a service client using `name` as the stable entity ID.
    #[track_caller]
    fn create_service_client_for_name<
        'entity,
        S: RosService<
                Request: nros_node::rmw_type_registry::MessageForRmw,
                Reply: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
    ) -> NodeResult<NodeServiceClient<'entity, S>> {
        self.create_service_client::<S>(EntityId::new(name), name)
    }

    /// Declare an action server. Stable action and callback IDs are required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_action_server<
        'entity,
        'callback,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        action_name: &str,
    ) -> NodeResult<NodeActionServer<'entity, A>> {
        self.create_action_server_with_callbacks::<A>(
            id,
            callback_id,
            callback_id,
            callback_id,
            action_name,
        )
    }

    /// Declare an action server with distinct goal/cancel/accepted callbacks.
    #[track_caller]
    #[doc(hidden)]
    fn create_action_server_with_callbacks<
        'entity,
        'goal,
        'cancel,
        'accepted,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        id: EntityId<'entity>,
        goal_callback_id: CallbackId<'goal>,
        cancel_callback_id: CallbackId<'cancel>,
        accepted_callback_id: CallbackId<'accepted>,
        action_name: &str,
    ) -> NodeResult<NodeActionServer<'entity, A>> {
        register_declared_action::<A>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::ActionServer,
            source_name: action_name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(goal_callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.action_cancel_callback_id = Some(copy_str(cancel_callback_id.as_str())?);
        metadata.action_cancel_source = metadata.callback_source.clone();
        metadata.action_accepted_callback_id = Some(copy_str(accepted_callback_id.as_str())?);
        metadata.action_accepted_source = metadata.callback_source.clone();
        metadata.source = metadata.callback_source.clone();
        self.__declare_entity(metadata)?;
        Ok(NodeActionServer::new(id))
    }

    /// Declare an action server using `name` as the stable entity ID and
    /// default goal/cancel/accepted callback ID.
    #[track_caller]
    fn create_action_server_for_name<
        'entity,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
    ) -> NodeResult<NodeActionServer<'entity, A>> {
        self.declare_action_server::<A>(EntityId::new(name), CallbackId::new(name), name)
    }

    /// Declare an action server using `name` as the stable entity ID and
    /// explicit source callback names for goal, cancel, and accepted events.
    #[track_caller]
    fn create_action_server_for_name_with_callbacks<
        'entity,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
        goal_callback_name: &str,
        cancel_callback_name: &str,
        accepted_callback_name: &str,
    ) -> NodeResult<NodeActionServer<'entity, A>> {
        self.create_action_server_with_callbacks::<A>(
            EntityId::new(name),
            CallbackId::new(goal_callback_name),
            CallbackId::new(cancel_callback_name),
            CallbackId::new(accepted_callback_name),
            name,
        )
    }

    /// Declare an action server whose stable entity and callback IDs are
    /// both synthesized from the action-name literal, returning an
    /// [`ActionTag`] the Node author stores on `Self::State` and matches
    /// against the `Callback<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// The synthesized callback ID is shared by the goal / cancel /
    /// accepted callbacks (matching the default behavior of
    /// [`declare_action_server`](Self::declare_action_server)).
    ///
    /// Tag-only registration is restricted to the SERVER side: clients
    /// need a USABLE handle (`NodeActionClient`) to dispatch goals, so
    /// use the existing
    /// [`create_action_client_for_name`](Self::create_action_client_for_name) builder
    /// for the client side.
    #[track_caller]
    fn create_action_static<
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'static str,
    ) -> NodeResult<ActionTag> {
        self.create_action_server_for_name::<A>(name)?;
        Ok(ActionTag::new(name))
    }

    /// Declare an action client. Stable action client ID is required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_action_client<
        'entity,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        id: EntityId<'entity>,
        action_name: &str,
    ) -> NodeResult<NodeActionClient<'entity, A>> {
        register_declared_action::<A>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::ActionClient,
            source_name: action_name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.__declare_entity(metadata)?;
        Ok(NodeActionClient::new(id))
    }

    /// Declare an action client using `name` as the stable entity ID.
    #[track_caller]
    fn create_action_client_for_name<
        'entity,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
    ) -> NodeResult<NodeActionClient<'entity, A>> {
        self.declare_action_client::<A>(EntityId::new(name), name)
    }

    /// Declare an action client that delivers the goal RESULT + FEEDBACK to
    /// named callbacks (Phase 212.M-F.23). `name` is the stable entity ID. The
    /// executor auto-drives accept → feedback stream → result during spin and
    /// dispatches `ExecutableNode::on_callback` with `result_callback_name`
    /// (payload = result CDR) on completion, and with `feedback_callback_name`
    /// (payload = feedback CDR) per feedback message. Read either with
    /// `CallbackCtx::message::<A::Result>()` / `::<A::Feedback>()`. Without
    /// these the client can only `send_goal`; result + feedback are dropped.
    ///
    /// (Layout note: the action-client variant reuses the server-side
    /// `action_accepted_callback_id` metadata slot for the feedback callback —
    /// that field is unused on a client, so no new schema field is needed.)
    #[track_caller]
    fn create_action_client_with_callbacks_for_name<
        'entity,
        A: RosAction<
                Goal: nros_node::rmw_type_registry::MessageForRmw,
                Result: nros_node::rmw_type_registry::MessageForRmw,
                Feedback: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalRequest: nros_node::rmw_type_registry::MessageForRmw,
                SendGoalResponse: nros_node::rmw_type_registry::MessageForRmw,
                GetResultRequest: nros_node::rmw_type_registry::MessageForRmw,
                GetResultResponse: nros_node::rmw_type_registry::MessageForRmw,
                FeedbackMessage: nros_node::rmw_type_registry::MessageForRmw,
            >,
    >(
        &mut self,
        name: &'entity str,
        result_callback_name: &str,
        feedback_callback_name: &str,
    ) -> NodeResult<NodeActionClient<'entity, A>> {
        register_declared_action::<A>()?;
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id: EntityId::new(name),
            node_id: self.__node_id()?,
            kind: EntityKind::ActionClient,
            source_name: name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QoSProfile::default(),
        })?;
        metadata.callback_id = Some(copy_str(result_callback_name)?);
        metadata.action_accepted_callback_id = Some(copy_str(feedback_callback_name)?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.__declare_entity(metadata)?;
        Ok(NodeActionClient::new(EntityId::new(name)))
    }

    /// Declare a parameter. Stable parameter ID is required.
    #[track_caller]
    #[doc(hidden)]
    fn declare_parameter<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        parameter_type: ParameterType,
    ) -> NodeResult<NodeParameter<'entity>> {
        self.declare_parameter_with_default(id, name, ParameterDefault::for_type(parameter_type)?)
    }

    /// Declare a parameter with a concrete source default.
    #[track_caller]
    #[doc(hidden)]
    fn declare_parameter_with_default<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        default: ParameterDefault,
    ) -> NodeResult<NodeParameter<'entity>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.__node_id()?,
            kind: EntityKind::Parameter,
            source_name: name,
            type_name: "",
            type_hash: "",
            qos: QoSProfile::default(),
        })?;
        metadata.parameter_type = Some(default.parameter_type());
        metadata.parameter_default = Some(default);
        metadata.source = SourceLocationMetadata::caller()?;
        self.__declare_entity(metadata)?;
        Ok(NodeParameter::new(id))
    }

    /// Declare a parameter using `name` as the generated stable entity ID.
    #[track_caller]
    fn declare_parameter_for_name<'entity>(
        &mut self,
        name: &'entity str,
        parameter_type: ParameterType,
    ) -> NodeResult<NodeParameter<'entity>> {
        self.declare_parameter(EntityId::new(name), name, parameter_type)
    }

    /// Declare a parameter with a concrete source default, using `name` as
    /// the generated stable entity ID.
    #[track_caller]
    fn declare_parameter_for_name_with_default<'entity>(
        &mut self,
        name: &'entity str,
        default: ParameterDefault,
    ) -> NodeResult<NodeParameter<'entity>> {
        self.declare_parameter_with_default(EntityId::new(name), name, default)
    }

    /// Record optional effects for a callback.
    #[doc(hidden)]
    fn callback<'callback>(&mut self, id: CallbackId<'callback>) -> CallbackEffects<'_, 'callback> {
        CallbackEffects {
            executor: self.__executor_mut(),
            id,
        }
    }

    /// Record optional effects for a named callback without exposing
    /// `CallbackId` at the declaration site.
    fn callback_for_name<'callback>(
        &mut self,
        name: &'callback str,
    ) -> CallbackEffects<'_, 'callback> {
        self.callback(CallbackId::new(name))
    }
}

#[cfg(feature = "rmw-cffi")]
impl DeclarativeNode for crate::Node<'_, 'static> {
    fn __node_id(&mut self) -> NodeResult<NodeId<'static>> {
        let exec_id = self.id();
        frame_mut(self.__executor())?.stable_id_of(exec_id)
    }

    fn __declare_entity(&mut self, mut metadata: EntityMetadata) -> NodeResult<()> {
        let exec_id = self.id();
        let executor = self.__executor();
        let frame = frame_mut(executor)?;
        if metadata.callback_group.is_none() {
            metadata.callback_group = frame.group_of(exec_id);
        }
        frame.sink.create_entity(executor, metadata)
    }

    fn __executor_mut(&mut self) -> &mut crate::Executor<'static> {
        self.__executor()
    }
}

/// Builder for optional callback effect metadata.
#[cfg(feature = "rmw-cffi")]
pub struct CallbackEffects<'ctx, 'id> {
    executor: &'ctx mut crate::Executor<'static>,
    id: CallbackId<'id>,
}

#[cfg(feature = "rmw-cffi")]
impl<'ctx, 'id> CallbackEffects<'ctx, 'id> {
    /// Record that callback reads from an entity.
    #[doc(hidden)]
    pub fn reads(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        let frame = frame_mut(self.executor)?;
        frame.sink.record_callback_effect(
            self.executor,
            self.id,
            CallbackEffectKind::Reads,
            entity_id,
        )?;
        Ok(self)
    }

    /// Record that callback reads from a declared entity handle.
    pub fn reads_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.reads(entity.entity_id())
    }

    /// Record that callback publishes via an entity.
    #[doc(hidden)]
    pub fn publishes(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        let frame = frame_mut(self.executor)?;
        frame.sink.record_callback_effect(
            self.executor,
            self.id,
            CallbackEffectKind::Publishes,
            entity_id,
        )?;
        Ok(self)
    }

    /// Record that callback publishes via a declared entity handle.
    pub fn publishes_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.publishes(entity.entity_id())
    }

    /// Record that callback writes to an entity or parameter.
    #[doc(hidden)]
    pub fn writes(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        let frame = frame_mut(self.executor)?;
        frame.sink.record_callback_effect(
            self.executor,
            self.id,
            CallbackEffectKind::Writes,
            entity_id,
        )?;
        Ok(self)
    }

    /// Record that callback writes to a declared entity handle.
    pub fn writes_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.writes(entity.entity_id())
    }
}

/// A declared source-level entity handle that can be referenced by callback effects.
#[doc(hidden)]
pub trait DeclaredEntity {
    /// Stable entity ID for metadata and generated runtime lookup.
    fn entity_id(&self) -> EntityId<'_>;
}

macro_rules! component_handle {
    ($name:ident $(, $type_param:ident)?) => {
        /// Source-level component entity handle.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $name<'id $(, $type_param)?> {
            id: EntityId<'id>,
            _marker: PhantomData<($($type_param,)?)>,
        }

        impl<'id $(, $type_param)?> $name<'id $(, $type_param)?> {
            const fn new(id: EntityId<'id>) -> Self {
                Self {
                    id,
                    _marker: PhantomData,
                }
            }

            /// Stable entity ID.
            #[doc(hidden)]
            pub const fn id(&self) -> EntityId<'id> {
                self.id
            }
        }

        impl<'id $(, $type_param)?> DeclaredEntity for $name<'id $(, $type_param)?> {
            fn entity_id(&self) -> EntityId<'_> {
                self.id
            }
        }
    };
}

component_handle!(NodePublisher, M);
component_handle!(NodeSubscription, M);
component_handle!(NodeServiceServer, S);
component_handle!(NodeServiceClient, S);
component_handle!(NodeActionServer, A);
component_handle!(NodeActionClient, A);
component_handle!(NodeTimer);
component_handle!(NodeParameter);

// ============================================================================
// Phase 172 W.5.1 — executable component layer (callback bodies)
// ============================================================================
//
// The declarative `Node::register` above stays the planning/metadata SSOT.
// This layer binds *runnable* bodies: the generated runtime builds the
// component `State` once, then routes each fired callback to `on_callback` with
// a `CallbackCtx` that exposes the triggering payload + an immediate publish
// path. Publishers are self-contained transport handles
// (`EmbeddedRawPublisher::publish_raw(&self)`), so a body publishes immediately
// mid-spin with no executor re-entrancy and no deferred queue (causality
// preserved). Shared state across a component's callbacks is `&mut State`
// behind the generated runtime's `'static` storage — `no_std`, no `alloc`.

/// Resolves a component publisher by its stable [`EntityId`] for the
/// callback-body publish path (W.5.1).
///
/// The generated runtime implements this over its owned `'static` publishers;
/// metadata/discovery mode never constructs a [`CallbackCtx`], so it need not
/// implement this.
pub trait PublisherResolver {
    /// Publish raw CDR bytes through the publisher with this stable entity id.
    /// `Err(NodeDeclError::Runtime)` if no such publisher is registered or the
    /// transport rejects the write.
    fn publish_raw(&self, entity_id: &str, data: &[u8]) -> NodeResult<()>;
}

/// Where a service / action-result callback body writes its reply (W.5.3): the
/// generated trampoline lends a `buf`; the body fills it via
/// [`CallbackCtx::reply`] and the trampoline reads `*written` back out.
struct ReplySink<'a> {
    buf: &'a mut [u8],
    written: &'a mut usize,
}

/// Where an action goal / cancel-decision callback writes its accept/reject
/// (W.5.3): the generated trampoline lends the out-slot, the body fills it via
/// [`CallbackCtx::set_goal_response`] / [`set_cancel_response`](CallbackCtx::set_cancel_response),
/// and the trampoline returns it. Decisions need no executor — unlike feedback /
/// result, which do (see the action-execution note in Phase 172 W.5.3).
enum DecisionSink<'a> {
    Goal(&'a mut GoalResponse),
    Cancel(&'a mut CancelResponse),
}

/// Context handed to an executable component callback body (W.5.1).
///
/// Carries the triggering payload (raw CDR — empty for timers) plus the
/// publisher resolver, so a body can read its message and publish immediately.
/// Service / action-result callbacks additionally carry a `ReplySink` the body
/// fills via [`reply`](Self::reply); action goal / cancel callbacks carry a
/// `DecisionSink` the body fills via
/// [`set_goal_response`](Self::set_goal_response) /
/// [`set_cancel_response`](Self::set_cancel_response) (W.5.3).
/// issue 0461 — bytes of `unique_identifier_msgs/UUID` that precede the goal
/// fields in a SendGoal request. A fixed `uint8[16]` array: no length prefix.
const GOAL_UUID_LEN: usize = 16;

pub struct CallbackCtx<'a> {
    payload: &'a [u8],
    publishers: &'a dyn PublisherResolver,
    reply: Option<ReplySink<'a>>,
    decision: Option<DecisionSink<'a>>,
    /// Phase 250 (Wave 2) — E2E message-integrity status for a subscription that
    /// opted in via `.safety()`; `None` for every other callback (timers,
    /// services, non-safety subscriptions). Read with
    /// [`integrity`](Self::integrity). Gated with the capability so it is
    /// zero-cost when `safety-e2e` is off.
    #[cfg(feature = "safety-e2e")]
    integrity: Option<&'a crate::IntegrityStatus>,
    /// Phase 264 W4c — the executor's volatile parameter store, threaded by the dispatch
    /// site from the component cell (`None` until `[param_services]` registers the store).
    /// Read with [`parameter`](Self::parameter). Gated so it is zero-cost when
    /// `param-services` is off.
    #[cfg(feature = "param-services")]
    params: Option<&'a crate::ParameterServer<'a>>,
    /// phase-426 W3 — WHICH node's parameters `parameter()` reads.
    ///
    /// One set of six parameter services is registered per node and the store
    /// is keyed the same way, so a context that carries the store without the
    /// key can only ever answer for one node — which is how every component on
    /// a multi-node executor came to read the primary's values.
    #[cfg(feature = "param-services")]
    param_node: nros_params::NodeKey,
}

impl<'a> CallbackCtx<'a> {
    /// Build a callback context with no reply sink (timer / subscription).
    /// `payload` is the entity's raw CDR (empty slice for timers).
    pub fn new(payload: &'a [u8], publishers: &'a dyn PublisherResolver) -> Self {
        Self {
            payload,
            publishers,
            reply: None,
            decision: None,
            #[cfg(feature = "safety-e2e")]
            integrity: None,
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Phase 250 (Wave 2) — build a subscription context carrying E2E
    /// [`IntegrityStatus`](crate::IntegrityStatus) (the declarative `.safety()`
    /// path). The body reads both the message ([`message`](Self::message)) and
    /// the status ([`integrity`](Self::integrity)) in one callback, mirroring the
    /// imperative `FnMut(&M, &IntegrityStatus)` shape.
    #[cfg(feature = "safety-e2e")]
    pub fn new_with_integrity(
        payload: &'a [u8],
        publishers: &'a dyn PublisherResolver,
        integrity: &'a crate::IntegrityStatus,
    ) -> Self {
        Self {
            payload,
            publishers,
            reply: None,
            decision: None,
            integrity: Some(integrity),
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Build a callback context with a reply sink (service / action-result;
    /// W.5.3). The body fills `reply_buf` via [`reply`](Self::reply); the
    /// generated trampoline reads `*reply_written` back as the response length.
    pub fn with_reply(
        payload: &'a [u8],
        publishers: &'a dyn PublisherResolver,
        reply_buf: &'a mut [u8],
        reply_written: &'a mut usize,
    ) -> Self {
        *reply_written = 0;
        Self {
            payload,
            publishers,
            reply: Some(ReplySink {
                buf: reply_buf,
                written: reply_written,
            }),
            decision: None,
            #[cfg(feature = "safety-e2e")]
            integrity: None,
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Build a context for an action **goal** callback (W.5.3): the body decides
    /// accept/reject via [`set_goal_response`](Self::set_goal_response); the
    /// generated trampoline returns `*out`. `payload` is the goal CDR.
    pub fn with_goal_decision(
        payload: &'a [u8],
        publishers: &'a dyn PublisherResolver,
        out: &'a mut GoalResponse,
    ) -> Self {
        Self {
            payload,
            publishers,
            reply: None,
            decision: Some(DecisionSink::Goal(out)),
            #[cfg(feature = "safety-e2e")]
            integrity: None,
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Build a context for an action **cancel** callback (W.5.3): the body decides
    /// accept/reject via [`set_cancel_response`](Self::set_cancel_response).
    pub fn with_cancel_decision(
        payload: &'a [u8],
        publishers: &'a dyn PublisherResolver,
        out: &'a mut CancelResponse,
    ) -> Self {
        Self {
            payload,
            publishers,
            reply: None,
            decision: Some(DecisionSink::Cancel(out)),
            #[cfg(feature = "safety-e2e")]
            integrity: None,
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Phase 264 W4c — thread the executor's volatile parameter store into this context.
    /// The dispatch site calls this after construction (the store reaches the callback via
    /// the component cell, not the constructor args). No-op-equivalent when `None`.
    #[cfg(feature = "param-services")]
    pub fn set_param_server(
        &mut self,
        params: Option<&'a crate::ParameterServer<'a>>,
        node: nros_params::NodeKey,
    ) {
        self.params = params;
        self.param_node = node;
    }

    /// Phase 264 W4c — read this node's parameter `name` as `T`, or `None` if the
    /// parameter is undeclared, the wrong type, or `[param_services]` is not enabled.
    /// Returns the **live** value — the launch-baked initial, or whatever a `ros2 param
    /// set` last wrote (RFC-0004 §10; values are volatile, lost at the next boot).
    #[cfg(feature = "param-services")]
    pub fn parameter<T: crate::ParameterVariant>(&self, name: &str) -> Option<T> {
        self.params
            // phase-426 W3 — the store is keyed by node, and the key is the
            // component's OWN node, threaded in beside the store by the
            // dispatch site. Reading PRIMARY here (which is what this did
            // until W3) makes the second node on an executor read its
            // sibling's values.
            .and_then(|server| server.get(self.param_node, name))
            .and_then(T::from_parameter_value)
    }

    /// Set the action goal-callback's accept/reject decision (W.5.3). `Err` when
    /// the callback is not a goal decision.
    pub fn set_goal_response(&mut self, response: GoalResponse) -> NodeResult<()> {
        match &mut self.decision {
            Some(DecisionSink::Goal(slot)) => {
                **slot = response;
                Ok(())
            }
            _ => Err(NodeDeclError::Runtime),
        }
    }

    /// Set the action cancel-callback's accept/reject decision (W.5.3). `Err` when
    /// the callback is not a cancel decision.
    ///
    /// Issue 0796 — `CancelResponse` here is the PER-GOAL decision
    /// (`Reject` / `Accept`), the twin of [`GoalResponse`] and the same two
    /// values C's `nros_cancel_response_t` and C++'s `nros::CancelResponse`
    /// carry. It used to be the `action_msgs/srv/CancelGoal` RPC return code
    /// (now `nros_core::CancelReturnCode`), so answering "cancel this goal"
    /// was spelled `CancelResponse::Ok` — a whole-request status code used to
    /// decide one goal.
    pub fn set_cancel_response(&mut self, response: CancelResponse) -> NodeResult<()> {
        match &mut self.decision {
            Some(DecisionSink::Cancel(slot)) => {
                **slot = response;
                Ok(())
            }
            _ => Err(NodeDeclError::Runtime),
        }
    }

    /// Write the service / action reply as raw CDR bytes (W.5.3). `Err` when the
    /// callback has no reply sink (timer / subscription) or the reply exceeds the
    /// lent buffer.
    pub fn reply_raw(&mut self, data: &[u8]) -> NodeResult<()> {
        let sink = self.reply.as_mut().ok_or(NodeDeclError::Runtime)?;
        if data.len() > sink.buf.len() {
            return Err(NodeDeclError::Runtime);
        }
        sink.buf[..data.len()].copy_from_slice(data);
        *sink.written = data.len();
        Ok(())
    }

    /// Serialize `msg` and write it as the service / action reply (W.5.3).
    pub fn reply<M: RosMessage, const N: usize>(&mut self, msg: &M) -> NodeResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| NodeDeclError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.reply_raw(&buf[..len])
    }

    /// Raw CDR payload of the triggering message / request. Empty for timers.
    pub fn payload(&self) -> &[u8] {
        self.payload
    }

    /// Phase 250 (Wave 2) — E2E message-integrity status (CRC + sequence gap/dup)
    /// for this dispatch. `Some` only when the firing subscription opted in via
    /// `.safety()`; `None` for timers, services, and non-safety subscriptions.
    /// Read it alongside [`message`](Self::message) — the status describes the
    /// message you just received.
    #[cfg(feature = "safety-e2e")]
    pub fn integrity(&self) -> Option<&crate::IntegrityStatus> {
        self.integrity
    }

    /// Deserialize the triggering payload as `M` (subscription / service-request
    /// bodies). `Err` if the payload is malformed for `M`.
    pub fn message<M: RosMessage>(&self) -> NodeResult<M> {
        let mut reader =
            crate::CdrReader::new_with_header(self.payload).map_err(|_| NodeDeclError::Runtime)?;
        // issue 0461 — an action GOAL callback's payload is the whole SendGoal
        // request, `[CDR header][goal_id uuid][goal fields]`. Without this skip
        // the reader is sitting on the uuid and a goal type decodes its first
        // four bytes — the goal counter, so every goal looked like `order = 1`.
        //
        // The goal_id reaches the callback by other means (the server's
        // `for_each_active_goal_for_name`), so it is framing here, not data.
        // Same shape as the typed `try_accept_goal` path, which has always
        // skipped it and has always decoded correctly.
        if matches!(self.decision, Some(DecisionSink::Goal(_))) {
            for _ in 0..GOAL_UUID_LEN {
                let _ = reader.read_u8();
            }
        }
        M::deserialize(&mut reader).map_err(|_| NodeDeclError::Runtime)
    }

    /// Publish raw CDR bytes through the named publisher entity (immediate).
    #[doc(hidden)]
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> NodeResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
    /// `N` must be ≥ the CDR-encoded size of `msg`; the generated runtime picks
    /// it from the message type.
    #[doc(hidden)]
    pub fn publish<M: RosMessage, const N: usize>(
        &self,
        publisher: EntityId<'_>,
        msg: &M,
    ) -> NodeResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| NodeDeclError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.publish_raw(publisher, &buf[..len])
    }

    /// Serialize `msg` and publish through the entity synthesized from `topic`.
    ///
    /// This pairs with
    /// [`DeclarativeNode::create_publisher_for_topic`], allowing simple callback
    /// bodies to use the ROS topic literal instead of restating an unrelated
    /// stable entity ID.
    pub fn publish_to_topic<M: RosMessage, const N: usize>(
        &self,
        topic: &str,
        msg: &M,
    ) -> NodeResult<()> {
        self.publish::<M, N>(EntityId::new(topic), msg)
    }
}

/// The executable counterpart of [`Component`] (W.5.1).
///
/// `register` (declarative) stays the planning SSOT; this binds runnable
/// bodies. The generated runtime builds [`State`](ExecutableNode::State) once via
/// [`init`](ExecutableNode::init), then routes every fired callback to
/// [`on_callback`](ExecutableNode::on_callback). Trait-dispatch (no boxed `dyn`, no
/// `alloc`) keeps it `no_std`.
/// Executor-backed action operations a [`TickCtx`] drives (W.5.6).
///
/// Action result/feedback need `&mut Executor` (`complete_goal_raw` /
/// `publish_feedback_raw`), which a mid-spin *callback* can't hold (the executor
/// is borrowed) — so they run from [`ExecutableNode::tick`], between spins.
/// The generated runtime implements this over the real executor + the action
/// servers' handles (resolved by stable action entity id); the component never
/// sees the executor directly. Kept as a trait so [`TickCtx`] stays `no_std` +
/// free of the `rmw-cffi`-gated `Executor` type.
pub trait ActionExecutor {
    /// Complete the goal `goal_id` on action `action_entity` with raw CDR result.
    fn complete_goal_raw(
        &mut self,
        action_entity: &str,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &[u8],
    ) -> NodeResult<()>;

    /// Publish raw CDR feedback for `goal_id` on action `action_entity`.
    fn publish_feedback_raw(
        &mut self,
        action_entity: &str,
        goal_id: &GoalId,
        feedback: &[u8],
    ) -> NodeResult<()>;

    /// Visit every goal on `action_entity` that has been accepted but not yet
    /// completed, with its id + current status. The execution seam: a `tick` body
    /// has no other way to learn an accepted goal's id (the goal-decision callback
    /// doesn't surface it), so it iterates here to drive feedback / completion.
    fn for_each_active_goal(&self, action_entity: &str, visit: &mut dyn FnMut(&GoalId, GoalStatus));
}

/// Executor-backed CLIENT operations a [`TickCtx`] drives (Phase 212.M-F.4).
///
/// Service-client `call` + action-client `send_goal` need `&mut Executor`
/// (the W.5.6 client handles live on the executor), which a mid-spin
/// callback can't hold. They run from [`ExecutableNode::tick`], between
/// spins. The generated runtime impls this over the real executor + the
/// service/action client handles (resolved by stable client entity id); the
/// component never sees the executor directly. Kept as a trait so [`TickCtx`]
/// stays `no_std` + free of the `rmw-cffi`-gated `Executor` type.
///
/// Mirrors the sibling [`ActionExecutor`] (server-side ops). Splitting
/// client vs server keeps each trait small + lets the codegen-side
/// `GenClientDispatch` impl resolve client handles independently from
/// server handles.
pub trait ClientDispatch {
    /// Issue a service-client request on `service_entity` carrying CDR
    /// `request_cdr`; block on the reply, write the response CDR into
    /// `response_buf`, return the response length in bytes.
    ///
    /// The synchronous block is built on the executor-driven
    /// `send_request_raw` + `take_response_raw` pair (phase-301: the
    /// RMW layer has no blocking call) — the tick hook drives the
    /// executor between callback dispatch, so a blocked `call_raw`
    /// does not starve other callbacks (each tick yields back to the
    /// runtime after returning).
    fn call_raw(
        &mut self,
        service_entity: &str,
        request_cdr: &[u8],
        response_buf: &mut [u8],
    ) -> NodeResult<usize>;

    /// Send an action-client goal request on `action_entity` carrying
    /// CDR `goal_cdr`; return the assigned [`GoalId`] (server-stamped on
    /// the goal-accept response). Result + feedback streams arrive via
    /// callback dispatch — not this method.
    fn send_goal_raw(&mut self, action_entity: &str, goal_cdr: &[u8]) -> NodeResult<GoalId>;

    /// Whether a server for the service client `service_entity` is CURRENTLY
    /// discoverable — `rclcpp::ClientBase::service_is_ready`, for a tick.
    ///
    /// phase-428 W13. Three answers: `Ok(true)`, `Ok(false)`, or `Err` when
    /// the backend cannot say (no discovery channel, or the entity is
    /// unknown). A tick that gates its first `call_raw` on `Ok(true)` and
    /// treats `Err` as "call anyway" keeps the request-is-the-probe behaviour
    /// on backends without discovery and stops sending into the void on the
    /// ones with it.
    ///
    /// Default `Err(NodeDeclError::Runtime)` — "cannot say" — so an
    /// implementor that predates this method (the orchestration codegen's
    /// dispatch) keeps its shape.
    fn service_is_ready(&self, service_entity: &str) -> NodeResult<bool> {
        let _ = service_entity;
        Err(NodeDeclError::Runtime)
    }
}

/// Context handed to [`ExecutableNode::tick`] (W.5.6 + M-F.4): the per-spin
/// hook that runs *between* callback dispatch, where the executor is free.
/// Exposes the immediate publish path (like `CallbackCtx`) plus executor-backed
/// action-server ops (complete goal / publish feedback) AND executor-backed
/// client-side ops (service `call` / action-client `send_goal`). Callbacks
/// can't perform any of these since they don't hold the executor.
pub struct TickCtx<'a> {
    publishers: &'a dyn PublisherResolver,
    actions: &'a mut dyn ActionExecutor,
    clients: &'a mut dyn ClientDispatch,
    /// Phase 264 W4c — the executor's volatile parameter store, threaded by the tick
    /// driver (`tick_one_cell` already holds the executor). `None` until
    /// `[param_services]` registers the store. Read with [`parameter`](Self::parameter).
    #[cfg(feature = "param-services")]
    params: Option<&'a crate::ParameterServer<'a>>,
    /// phase-426 W3 — WHICH node's parameters `parameter()` reads.
    ///
    /// One set of six parameter services is registered per node and the store
    /// is keyed the same way, so a context that carries the store without the
    /// key can only ever answer for one node — which is how every component on
    /// a multi-node executor came to read the primary's values.
    #[cfg(feature = "param-services")]
    param_node: nros_params::NodeKey,
}

impl<'a> TickCtx<'a> {
    /// Build a tick context (called by the generated runtime each spin).
    pub fn new(
        publishers: &'a dyn PublisherResolver,
        actions: &'a mut dyn ActionExecutor,
        clients: &'a mut dyn ClientDispatch,
    ) -> Self {
        Self {
            publishers,
            actions,
            clients,
            #[cfg(feature = "param-services")]
            params: None,
            // phase-426 W3 — the store is keyed by node and this context does
            // not know which one until the dispatch site says so
            // (`set_param_server`). PRIMARY until then, which is the executor's
            // first node and what a single-node image means.
            #[cfg(feature = "param-services")]
            param_node: nros_params::NodeKey::PRIMARY,
        }
    }

    /// Phase 264 W4c — thread the executor's volatile parameter store in (the tick
    /// driver holds the executor directly). No-op-equivalent when `None`.
    #[cfg(feature = "param-services")]
    pub fn set_param_server(
        &mut self,
        params: Option<&'a crate::ParameterServer<'a>>,
        node: nros_params::NodeKey,
    ) {
        self.params = params;
        self.param_node = node;
    }

    /// Phase 264 W4c — read this node's parameter `name` as `T` during `tick`, or `None`
    /// if undeclared, the wrong type, or `[param_services]` is off. Returns the live
    /// value (baked initial or last `ros2 param set`; volatile — RFC-0004 §10).
    #[cfg(feature = "param-services")]
    pub fn parameter<T: crate::ParameterVariant>(&self, name: &str) -> Option<T> {
        self.params
            // phase-426 W3 — the store is keyed by node, and the key is the
            // component's OWN node, threaded in beside the store by the
            // dispatch site. Reading PRIMARY here (which is what this did
            // until W3) makes the second node on an executor read its
            // sibling's values.
            .and_then(|server| server.get(self.param_node, name))
            .and_then(T::from_parameter_value)
    }

    /// Publish raw CDR bytes through the named publisher entity (immediate).
    #[doc(hidden)]
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> NodeResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
    #[doc(hidden)]
    pub fn publish<M: RosMessage, const N: usize>(
        &self,
        publisher: EntityId<'_>,
        msg: &M,
    ) -> NodeResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| NodeDeclError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.publish_raw(publisher, &buf[..len])
    }

    /// Serialize `msg` and publish through the entity synthesized from `topic`.
    ///
    /// This pairs with [`DeclarativeNode::create_publisher_for_topic`] for
    /// executable tick hooks.
    pub fn publish_to_topic<M: RosMessage, const N: usize>(
        &self,
        topic: &str,
        msg: &M,
    ) -> NodeResult<()> {
        self.publish::<M, N>(EntityId::new(topic), msg)
    }

    /// Complete an action goal with a typed result (W.5.6 — needs the executor,
    /// hence tick-only).
    #[doc(hidden)]
    pub fn complete_goal<R: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &R,
    ) -> NodeResult<()> {
        // RFC-0069 / issue 0418 — NO inner encapsulation header. ROS 2's
        // `<Action>_GetResult_Response` is ONE CDR message: `[header][status][result
        // fields]`. This used to write a second header inside the envelope, which
        // made the payload `[outer][status][pad][INNER][fields]` — self-consistent
        // with nano-ros's own raw consumer and undecodable by any `rcl_action` peer
        // or by nano-ros's TYPED path.
        //
        // The issue-#35 corruption that motivated the old header ("the reader eats
        // the first data word … `sequence` deserialized to len 0") is now prevented
        // on the READ side instead: the executor splices the envelope's encap onto
        // the body before the callback sees it, so `CallbackCtx::message` still
        // receives a well-formed CDR message. Producer and consumer changed
        // together — either alone reproduces #35.
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        result
            .serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.actions
            .complete_goal_raw(action.as_str(), goal_id, status, &buf[..len])
    }

    /// Complete an action goal on the action entity synthesized from `name`.
    ///
    /// This pairs with
    /// [`DeclarativeNode::create_action_server_for_name`] and
    /// [`DeclarativeNode::create_action_server_for_name_with_callbacks`].
    pub fn complete_goal_for_name<R: RosMessage, const N: usize>(
        &mut self,
        name: &str,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &R,
    ) -> NodeResult<()> {
        self.complete_goal::<R, N>(EntityId::new(name), goal_id, status, result)
    }

    /// Visit each active (accepted, not yet completed) goal on `action` with its
    /// id + status — how a `tick` body discovers goals to feed / complete. Collect
    /// the ids you want to act on, then call [`Self::publish_feedback`] /
    /// [`Self::complete_goal`] after the visit returns (those borrow `self`
    /// mutably, so they can't run inside `visit`).
    #[doc(hidden)]
    pub fn for_each_active_goal(
        &self,
        action: EntityId<'_>,
        visit: &mut dyn FnMut(&GoalId, GoalStatus),
    ) {
        self.actions.for_each_active_goal(action.as_str(), visit);
    }

    /// Visit active goals on the action entity synthesized from `name`.
    pub fn for_each_active_goal_for_name(
        &self,
        name: &str,
        visit: &mut dyn FnMut(&GoalId, GoalStatus),
    ) {
        self.for_each_active_goal(EntityId::new(name), visit);
    }

    /// Publish typed feedback for an active action goal (W.5.6 — tick-only).
    #[doc(hidden)]
    pub fn publish_feedback<F: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal_id: &GoalId,
        feedback: &F,
    ) -> NodeResult<()> {
        // RFC-0069 / issue 0418 — NO inner encapsulation header; see
        // `complete_goal` above. ROS 2's `<Action>_FeedbackMessage` is ONE CDR
        // message: `[header][goal_id][feedback fields]`. The executor frames
        // `[header][goal_id]` and this payload is the fields alone.
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        feedback
            .serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.actions
            .publish_feedback_raw(action.as_str(), goal_id, &buf[..len])
    }

    /// Publish feedback on the action entity synthesized from `name`.
    pub fn publish_feedback_for_name<F: RosMessage, const N: usize>(
        &mut self,
        name: &str,
        goal_id: &GoalId,
        feedback: &F,
    ) -> NodeResult<()> {
        self.publish_feedback::<F, N>(EntityId::new(name), goal_id, feedback)
    }

    /// Issue a service-client raw-CDR request and block on the reply
    /// (M-F.4 — tick-only). Writes the response CDR into `response_buf`
    /// and returns the response length in bytes.
    #[doc(hidden)]
    pub fn call_raw(
        &mut self,
        service: EntityId<'_>,
        request_cdr: &[u8],
        response_buf: &mut [u8],
    ) -> NodeResult<usize> {
        self.clients
            .call_raw(service.as_str(), request_cdr, response_buf)
    }

    /// Whether a server for the service client `service` is CURRENTLY
    /// discoverable (M-F.4 — tick-only). Mirrors
    /// `rclcpp::ClientBase::service_is_ready`; see
    /// [`ClientDispatch::service_is_ready`] for the three answers. There is no
    /// blocking `wait_for_service` here because a tick must not block the
    /// executor that drives it — check once per tick and return, which is the
    /// rclcpp `while (!wait_for_service(1s)) { "waiting again..." }` idiom
    /// with the timer as the cadence (phase-428 W13).
    #[doc(hidden)]
    pub fn service_is_ready(&self, service: EntityId<'_>) -> NodeResult<bool> {
        self.clients.service_is_ready(service.as_str())
    }

    /// [`Self::service_is_ready`] through the entity synthesized from `name`.
    pub fn service_is_ready_for_name(&self, name: &str) -> NodeResult<bool> {
        self.service_is_ready(EntityId::new(name))
    }

    /// Issue a raw service-client request through the entity synthesized
    /// from `name`.
    pub fn call_raw_for_name(
        &mut self,
        name: &str,
        request_cdr: &[u8],
        response_buf: &mut [u8],
    ) -> NodeResult<usize> {
        self.call_raw(EntityId::new(name), request_cdr, response_buf)
    }

    /// Issue a typed service-client request and decode the reply
    /// (M-F.4 — tick-only). `REQ_N` / `RESP_N` stack-size the request /
    /// response CDR buffers; size them via
    /// `<<Req as RosMessage>::SerializedSize as nros::SerializedSize>::SIZE`.
    #[doc(hidden)]
    pub fn call<Req: RosMessage, Resp: RosMessage, const REQ_N: usize, const RESP_N: usize>(
        &mut self,
        service: EntityId<'_>,
        request: &Req,
    ) -> NodeResult<Resp> {
        let mut req_buf = [0u8; REQ_N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut req_buf).map_err(|_| NodeDeclError::Runtime)?;
        request
            .serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let req_len = writer.position();

        let mut resp_buf = [0u8; RESP_N];
        let resp_len =
            self.clients
                .call_raw(service.as_str(), &req_buf[..req_len], &mut resp_buf)?;

        let mut reader = crate::CdrReader::new_with_header(&resp_buf[..resp_len])
            .map_err(|_| NodeDeclError::Runtime)?;
        Resp::deserialize(&mut reader).map_err(|_| NodeDeclError::Runtime)
    }

    /// Issue a typed service-client request through the entity synthesized
    /// from `name`.
    pub fn call_for_name<
        Req: RosMessage,
        Resp: RosMessage,
        const REQ_N: usize,
        const RESP_N: usize,
    >(
        &mut self,
        name: &str,
        request: &Req,
    ) -> NodeResult<Resp> {
        self.call::<Req, Resp, REQ_N, RESP_N>(EntityId::new(name), request)
    }

    /// Send a raw-CDR action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). Result + feedback streams arrive
    /// via callback dispatch; this method only kicks off the request.
    #[doc(hidden)]
    pub fn send_goal_raw(&mut self, action: EntityId<'_>, goal_cdr: &[u8]) -> NodeResult<GoalId> {
        self.clients.send_goal_raw(action.as_str(), goal_cdr)
    }

    /// Send a raw-CDR action-client goal through the entity synthesized
    /// from `name`.
    pub fn send_goal_raw_for_name(&mut self, name: &str, goal_cdr: &[u8]) -> NodeResult<GoalId> {
        self.send_goal_raw(EntityId::new(name), goal_cdr)
    }

    /// Send a typed action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). `N` stack-sizes the goal CDR
    /// buffer.
    #[doc(hidden)]
    pub fn send_goal<G: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal: &G,
    ) -> NodeResult<GoalId> {
        // RFC-0069 / issue 0418 — NO inner encapsulation header, same rule as
        // `publish_feedback` / `complete_goal`. ROS 2's `<Action>_SendGoal_Request`
        // is ONE CDR message: `[header][goal_id][goal fields]`. `send_goal_raw`
        // frames `[header][goal_id]`, so this payload is the fields alone.
        //
        // Issue 0448: this path used `new_with_header` and shipped a SECOND
        // encapsulation (`header|uuid|header|fields`) — 4 bytes over the ROS 2
        // layout. Fast-DDS sizes its reader history from the type and drops the
        // sample outright ("Change payload size of '28' bytes is larger than the
        // history payload size of '27'"), so the goal never reached the server
        // and the client saw a zeroed default result. nros-c / nros-cpp already
        // stripped the header; only this Rust path was missed.
        //
        // Confirmed independently while root-causing issue 0461: the extra
        // header also made every SERVER decode the wrong offset over zenoh
        // (C/C++ read the inner header as the first field, so an order of 10
        // arrived as 256). Same defect, a second symptom.
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        goal.serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.clients.send_goal_raw(action.as_str(), &buf[..len])
    }

    /// Send a typed action-client goal through the entity synthesized
    /// from `name`.
    pub fn send_goal_for_name<G: RosMessage, const N: usize>(
        &mut self,
        name: &str,
        goal: &G,
    ) -> NodeResult<GoalId> {
        self.send_goal::<G, N>(EntityId::new(name), goal)
    }
}

#[cfg(feature = "rmw-cffi")]
pub trait ExecutableNode: Component {
    /// Per-instance mutable state shared across the component's callbacks.
    type State;

    /// Build the initial state (called once by the generated runtime).
    fn init() -> Self::State;

    /// Run the body for `callback`. `ctx` exposes the triggering payload + the
    /// immediate publish path. Bodies match on the source callback name declared
    /// by `create_*_for_callback_name` and related helpers.
    fn on_callback(state: &mut Self::State, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>);

    /// Per-spin execution hook (W.5.6), run *between* callback dispatch by the
    /// generated runtime — where the executor is free, so this is the only place
    /// a component can complete action goals / publish feedback (via `ctx`) or do
    /// periodic work. Default: no-op (timer/sub/service-only components).
    fn tick(_state: &mut Self::State, _ctx: &mut TickCtx<'_>) {}
}

/// Emit a no-op [`ExecutableNode`] impl for a declarative-only component
/// (W.5.1). The generated runtime calls `on_callback` unconditionally, so a
/// component instantiated into a generated binary must impl `ExecutableNode`;
/// components without callback bodies use this to satisfy that contract:
///
/// ```ignore
/// pub struct Node;
/// impl nros::Component for Node { /* register(...) */ }
/// nros::declarative_component!(Node);
/// ```
#[macro_export]
macro_rules! declarative_component {
    ($ty:ty) => {
        impl $crate::ExecutableNode for $ty {
            type State = ();
            fn init() -> Self::State {}
            fn on_callback(
                _state: &mut Self::State,
                _callback: $crate::Callback<'_>,
                _ctx: &mut $crate::CallbackCtx<'_>,
            ) {
            }
        }
    };
}

/// Run component registration against a metadata recorder, on `executor`.
///
/// phase-483 W3: registration creates real nodes, so it needs an executor even
/// on the probe's road — open one on the `metadata` backend
/// (`ExecutorConfig::new("").rmw("metadata")`). The recorder receives every
/// declaration exactly as it did when it was the whole runtime.
#[cfg(feature = "rmw-cffi")]
pub fn register_node<C: Component>(
    recorder: &mut dyn NodeRuntime,
    executor: &mut crate::Executor<'static>,
) -> NodeResult<()> {
    let mut sink = RecordSink { recorder };
    let mut frame = ComponentFrame::new(&mut sink);
    let mut context = NodeContext::new(C::NAME, executor, &mut frame);
    C::register(&mut context)
}

/// Run `f` against a [`NodeContext`] whose declarations go to `recorder`,
/// on `executor` (opened on the `metadata` backend). The tests of the
/// declarative surface use it; the probe uses [`record_node_metadata`].
#[cfg(feature = "rmw-cffi")]
#[doc(hidden)]
pub fn __record_with_context<T>(
    component_name: &'static str,
    recorder: &mut dyn NodeRuntime,
    executor: &mut crate::Executor<'static>,
    f: impl FnOnce(&mut NodeContext<'_>) -> T,
) -> T {
    let mut sink = RecordSink { recorder };
    let mut frame = ComponentFrame::new(&mut sink);
    let mut context = NodeContext::new(component_name, executor, &mut frame);
    f(&mut context)
}

/// Phase 212.M.5.a.4 internal — `Box`-erase a freshly built component
/// `State` to the type-erased `*mut ()` ABI the BSP path uses. Called
/// only from the `nros::node!()` macro emit; not public API.
///
/// The returned pointer is a leaked `Box`; the BSP runtime keeps it
/// alive for the firmware lifetime (embedded slots never deallocate).
#[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
#[doc(hidden)]
pub fn __private_node_state_into_raw<C: ExecutableNode>(state: C::State) -> *mut () {
    extern crate alloc;
    alloc::boxed::Box::into_raw(alloc::boxed::Box::new(state)) as *mut ()
}

/// Run component registration against an in-memory metadata recorder, on an
/// executor the caller opened on the `metadata` backend.
#[cfg(feature = "rmw-cffi")]
pub fn record_node_metadata<C: Component>(
    recorder: &mut dyn NodeRuntime,
    executor: &mut crate::Executor<'static>,
) -> NodeResult<()> {
    register_node::<C>(recorder, executor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CdrReader, CdrWriter, DeserError, SerError};

    #[derive(Debug, Clone, Copy, Default)]
    struct TestMsg;

    impl crate::Serialize for TestMsg {
        fn serialize(&self, _writer: &mut CdrWriter) -> Result<(), SerError> {
            Ok(())
        }
    }

    impl crate::Deserialize for TestMsg {
        fn deserialize(_reader: &mut CdrReader) -> Result<Self, DeserError> {
            Ok(Self)
        }
    }

    impl RosMessage for TestMsg {
        const TYPE_NAME: &'static str = "test_msgs::msg::dds_::Test_";
        const TYPE_HASH: &'static str = "test_hash";
    }

    // Phase 380 W4 — `MessageForRmw` now also requires a field schema, because
    // that is where a subscription's size bound comes from. `TestMsg` is an
    // empty struct, so its schema is the empty slice and its bound is just the
    // encapsulation header — which is exactly what the build assertion should
    // see for it.
    impl nros_serdes::schema::Message for TestMsg {
        const TYPE_NAME: &'static str = "test_msgs/msg/Test";
        const FIELDS: &'static [nros_serdes::schema::Field] = &[];
    }

    #[cfg(feature = "rmw-cffi")]
    struct TalkerComponent;

    #[cfg(feature = "rmw-cffi")]
    impl Component for TalkerComponent {
        const NAME: &'static str = "talker_component";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node =
                context.create_node_with_id(NodeId::new("node"), NodeOptions::new("talker"))?;
            let _publisher =
                node.declare_publisher::<TestMsg>(EntityId::new("pub_chatter"), "chatter")?;
            let _subscription = node.declare_subscription::<TestMsg>(
                EntityId::new("sub_cmd"),
                CallbackId::new("on_cmd"),
                "~/cmd",
            )?;
            let _timer = node.declare_timer(
                EntityId::new("timer_tick"),
                CallbackId::new("on_tick"),
                TimerDuration::from_millis(10),
            )?;
            let _parameter =
                node.declare_parameter(EntityId::new("param_gain"), "gain", ParameterType::Double)?;
            node.callback(CallbackId::new("on_tick"))
                .publishes(EntityId::new("pub_chatter"))?
                .writes(EntityId::new("param_gain"))?;
            Ok(())
        }
    }

    #[test]
    fn component_missing_export_error_message_is_clear() {
        assert_eq!(
            NodeDeclError::MissingExport.message(),
            "package has no exported nros component"
        );
    }

    // W.5.1 — an executable component callback runs its body: mutates state +
    // publishes immediately through the resolver (the substrate the generator
    // will wire). `TalkerComponent` already impls `Node` (declarative);
    // here it also impls `ExecutableNode`.
    #[cfg(feature = "rmw-cffi")]
    impl ExecutableNode for TalkerComponent {
        type State = u32;

        fn init() -> u32 {
            0
        }

        fn on_callback(state: &mut u32, callback: Callback<'_>, ctx: &mut CallbackCtx<'_>) {
            if callback.as_str() == "on_tick" {
                *state += 1;
                // Publish through the declared publisher entity.
                let _ = ctx.publish::<TestMsg, 64>(EntityId::new("pub_chatter"), &TestMsg);
            }
        }
    }

    #[cfg(feature = "rmw-cffi")]
    #[test]
    fn executable_component_callback_publishes_and_mutates_state() {
        use core::cell::RefCell;

        struct RecordingResolver {
            last: RefCell<Option<(MetadataString, usize)>>,
        }
        impl PublisherResolver for RecordingResolver {
            fn publish_raw(&self, entity_id: &str, data: &[u8]) -> NodeResult<()> {
                *self.last.borrow_mut() = Some((copy_str(entity_id)?, data.len()));
                Ok(())
            }
        }

        let resolver = RecordingResolver {
            last: RefCell::new(None),
        };
        let mut state = TalkerComponent::init();
        let mut ctx = CallbackCtx::new(&[], &resolver);

        // An unrelated callback id does nothing.
        TalkerComponent::on_callback(
            &mut state,
            Callback::__from_id(CallbackId::new("other")),
            &mut ctx,
        );
        assert_eq!(state, 0);
        assert!(resolver.last.borrow().is_none());

        // The bound callback bumps state + publishes through "pub_chatter".
        TalkerComponent::on_callback(
            &mut state,
            Callback::__from_id(CallbackId::new("on_tick")),
            &mut ctx,
        );
        assert_eq!(state, 1);
        let last = resolver.last.borrow();
        let (entity, len) = last.as_ref().expect("a publish was recorded");
        assert_eq!(entity.as_str(), "pub_chatter");
        // Empty TestMsg ⇒ just the 4-byte CDR header.
        assert_eq!(*len, 4);
    }

    // W.5.3 — a service-style body writes its reply through the CallbackCtx
    // reply sink; the trampoline reads `*written` back. A timer/sub ctx (no
    // sink) rejects a reply.
    #[test]
    fn callback_ctx_reply_sink_roundtrips() {
        struct NoopResolver;
        impl PublisherResolver for NoopResolver {
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> NodeResult<()> {
                Ok(())
            }
        }
        let resolver = NoopResolver;
        let mut reply_buf = [0u8; 64];
        let mut written = 0usize;
        {
            let mut ctx = CallbackCtx::with_reply(&[], &resolver, &mut reply_buf, &mut written);
            ctx.reply::<TestMsg, 64>(&TestMsg).unwrap();
        }
        // Empty TestMsg ⇒ just the 4-byte CDR header.
        assert_eq!(written, 4);

        // A reply-less ctx (timer / subscription) rejects a reply.
        let mut ctx2 = CallbackCtx::new(&[], &resolver);
        assert!(ctx2.reply_raw(&[1, 2, 3]).is_err());
    }

    // Phase 264 W4c — a callback reads the live parameter value through
    // `CallbackCtx::parameter::<T>` once the dispatch site threads the store in.
    #[cfg(feature = "param-services")]
    #[test]
    fn callback_ctx_reads_param() {
        struct NoopResolver;
        impl PublisherResolver for NoopResolver {
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> NodeResult<()> {
                Ok(())
            }
        }
        let resolver = NoopResolver;

        // No store threaded ⇒ every read is None (param-services off / not registered).
        let ctx_none = CallbackCtx::new(&[], &resolver);
        assert_eq!(ctx_none.parameter::<i64>("speed"), None);

        // Seed a store with the typed value a `ros2 param set speed 7` would land on.
        // phase-382 W2' — the slots are the caller's; a local `ParameterStorage`
        // is the shape a test wants, sized for what it actually declares rather
        // than the build-time `MAX_PARAMETERS` default.
        // phase-426 W1/W3 — every store call names the node it is about.
        // `talker` is the executor's first node, `listener` its second.
        let talker = nros_params::NodeKey::PRIMARY;
        let listener = nros_params::NodeKey::new(1);
        let mut storage = crate::ParameterStorage::<4>::new();
        let mut server = crate::ParameterServer::new_in(storage.as_table());
        assert!(server.declare(talker, "speed", crate::ParameterValue::Integer(7)));
        assert!(server.declare(listener, "speed", crate::ParameterValue::Integer(99)));

        let mut ctx = CallbackCtx::new(&[], &resolver);
        ctx.set_param_server(Some(&server), talker);
        assert_eq!(ctx.parameter::<i64>("speed"), Some(7));
        // Wrong type ⇒ None, not a panic.
        assert_eq!(ctx.parameter::<bool>("speed"), None);
        // Undeclared ⇒ None.
        assert_eq!(ctx.parameter::<i64>("missing"), None);

        // phase-426 W3 — the SIBLING's identically-named parameter is a
        // different parameter, and the context reads the node it was given.
        // Before W3 this ctx was pinned to PRIMARY, so a listener component
        // read the talker's 7.
        let mut sibling = CallbackCtx::new(&[], &resolver);
        sibling.set_param_server(Some(&server), listener);
        assert_eq!(sibling.parameter::<i64>("speed"), Some(99));
    }

    // Phase 250 Wave 2 — the declarative `.safety()` surface: a normal ctx has
    // no integrity status; one built with `new_with_integrity` exposes it, read
    // alongside the message in the same callback (Shape A).
    #[cfg(feature = "safety-e2e")]
    #[test]
    fn callback_ctx_integrity_surface() {
        struct NoopResolver;
        impl PublisherResolver for NoopResolver {
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> NodeResult<()> {
                Ok(())
            }
        }
        let resolver = NoopResolver;

        // Non-safety dispatch (timer / plain sub) → None.
        let ctx = CallbackCtx::new(&[], &resolver);
        assert!(ctx.integrity().is_none());

        // Safety dispatch → the status rides alongside the payload.
        let status = crate::IntegrityStatus {
            gap: 2,
            duplicate: false,
            crc_valid: Some(true),
        };
        let ctx = CallbackCtx::new_with_integrity(&[], &resolver, &status);
        let got = ctx.integrity().expect("safety ctx carries status");
        assert_eq!(got.gap, 2);
        assert!(!got.duplicate);
        assert_eq!(got.crc_valid, Some(true));
    }

    // W.5.3 — an action goal / cancel body sets its accept/reject decision
    // through the CallbackCtx decision sink; the trampoline returns `*out`. A
    // wrong-kind setter (or a sink-less ctx) errors.
    #[test]
    fn callback_ctx_decision_sink() {
        struct NoopResolver;
        impl PublisherResolver for NoopResolver {
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> NodeResult<()> {
                Ok(())
            }
        }
        let resolver = NoopResolver;

        let mut gr = GoalResponse::Reject;
        {
            let mut ctx = CallbackCtx::with_goal_decision(&[], &resolver, &mut gr);
            ctx.set_goal_response(GoalResponse::AcceptAndExecute)
                .unwrap();
            // Wrong-kind setter on a goal ctx errors.
            assert!(ctx.set_cancel_response(CancelResponse::Accept).is_err());
        }
        assert!(matches!(gr, GoalResponse::AcceptAndExecute));

        let mut cr = CancelResponse::Reject;
        {
            let mut ctx = CallbackCtx::with_cancel_decision(&[], &resolver, &mut cr);
            ctx.set_cancel_response(CancelResponse::Accept).unwrap();
        }
        assert!(matches!(cr, CancelResponse::Accept));

        // A timer/sub ctx (no decision sink) rejects both.
        let mut ctx3 = CallbackCtx::new(&[], &resolver);
        assert!(ctx3.set_goal_response(GoalResponse::Reject).is_err());
        assert!(ctx3.set_cancel_response(CancelResponse::Accept).is_err());
    }

    // W.5.6 — the tick hook publishes (immediate) + drives executor-backed action
    // ops (complete goal / publish feedback) through the ActionExecutor seam.
    #[test]
    fn tick_ctx_publish_and_action_ops() {
        use core::cell::Cell;
        struct RecPub {
            published: Cell<bool>,
        }
        impl PublisherResolver for RecPub {
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> NodeResult<()> {
                self.published.set(true);
                Ok(())
            }
        }
        struct RecAct {
            completed: bool,
            fed: bool,
            visited: usize,
        }
        impl ActionExecutor for RecAct {
            fn complete_goal_raw(
                &mut self,
                _action_entity: &str,
                _goal_id: &GoalId,
                _status: GoalStatus,
                _result: &[u8],
            ) -> NodeResult<()> {
                self.completed = true;
                Ok(())
            }
            fn publish_feedback_raw(
                &mut self,
                _action_entity: &str,
                _goal_id: &GoalId,
                _feedback: &[u8],
            ) -> NodeResult<()> {
                self.fed = true;
                Ok(())
            }
            fn for_each_active_goal(
                &self,
                _action_entity: &str,
                visit: &mut dyn FnMut(&GoalId, GoalStatus),
            ) {
                // One pretend-active goal, so the tick body has something to drive.
                visit(&GoalId::zero(), GoalStatus::Executing);
            }
        }

        struct RecClients;
        impl ClientDispatch for RecClients {
            fn call_raw(
                &mut self,
                _service: &str,
                _req: &[u8],
                _resp: &mut [u8],
            ) -> NodeResult<usize> {
                Err(NodeDeclError::Runtime)
            }
            fn send_goal_raw(&mut self, _action: &str, _goal: &[u8]) -> NodeResult<GoalId> {
                Err(NodeDeclError::Runtime)
            }
        }

        let pubs = RecPub {
            published: Cell::new(false),
        };
        let mut acts = RecAct {
            completed: false,
            fed: false,
            visited: 0,
        };
        let mut clients = RecClients;
        let goal = GoalId::zero();
        let mut seen = 0usize;
        {
            let mut ctx = TickCtx::new(&pubs, &mut acts, &mut clients);
            ctx.publish::<TestMsg, 64>(EntityId::new("pub_x"), &TestMsg)
                .unwrap();
            // Discover the active goal the way a real tick body does, then act on it.
            ctx.for_each_active_goal(EntityId::new("act"), &mut |_id, _status| seen += 1);
            ctx.publish_feedback::<TestMsg, 64>(EntityId::new("act"), &goal, &TestMsg)
                .unwrap();
            ctx.complete_goal::<TestMsg, 64>(
                EntityId::new("act"),
                &goal,
                GoalStatus::Succeeded,
                &TestMsg,
            )
            .unwrap();
        }
        acts.visited = seen;
        assert!(pubs.published.get());
        assert!(acts.completed);
        assert!(acts.fed);
        assert_eq!(acts.visited, 1);
    }

    /// Phase 216.A.3 — `Node::DISPATCH` defaults to
    /// `DispatchStrategy::Inline` so every pre-216 `impl Node`
    /// keeps compiling unchanged.
    #[cfg(feature = "rmw-cffi")]
    #[test]
    fn node_dispatch_default_is_inline() {
        struct Dummy;
        impl Component for Dummy {
            const NAME: &'static str = "dummy";
            fn register(_: &mut NodeContext<'_>) -> NodeResult<()> {
                Ok(())
            }
        }
        assert_eq!(Dummy::DISPATCH, crate::DispatchStrategy::Inline);
    }

    // Phase 216.A.5 — `nros::node!()` emits the
    // `__nros_node_<pkg>_dispatch_strategy()` ABI export. We invoke the
    // macro on a dummy Node + ExecutableNode pair in a private sub-module
    // here so the macro expansion lives inside the `nros` crate itself;
    // the emitted `#[unsafe(no_mangle)] extern "C"` symbol is global, so
    // the test below re-declares + calls it. If the macro stopped
    // emitting the symbol (or renamed it) this would link-fail.
    //
    // `<pkg>` resolves to `CARGO_PKG_NAME` after
    // `sanitize_pkg_name_for_symbol`. The `nros` crate's pkg name is
    // literal `nros`, so the expected symbol is
    // `__nros_node_nros_dispatch_strategy`.
    // Phase 216 final wave — the macro emit now references
    // `::nros::Executor` (rmw-cffi-gated) in addition to the existing
    // alloc-gated `__private_node_state_into_raw`. Gate the test on
    // both features so the macro invocation only attempts to expand
    // when every referenced symbol is present.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi", feature = "macros"))]
    mod dispatch_probe_macro_test {
        // `extern crate self as nros;` at the crate root (in `lib.rs`,
        // `cfg(test)`-gated) lets the `::nros::*` paths the macro emits
        // resolve in-crate.
        use super::*;

        pub struct DispatchProbe;

        impl Component for DispatchProbe {
            const NAME: &'static str = "dispatch_probe";
            // Default `DISPATCH = Inline` ⇒ discriminant 0.
            fn register(_: &mut NodeContext<'_>) -> NodeResult<()> {
                Ok(())
            }
        }

        impl ExecutableNode for DispatchProbe {
            type State = ();
            fn init() -> Self::State {}
            fn on_callback(
                _state: &mut Self::State,
                _callback: Callback<'_>,
                _ctx: &mut CallbackCtx<'_>,
            ) {
            }
        }

        // Emits both the per-pkg `register` wrapper AND the new
        // `__nros_node_nros_dispatch_strategy` ABI symbol.
        nros_macros::node!(DispatchProbe);
    }

    // Also `macros`: this asserts the ABI symbol the `node!` invocation above
    // emits, so without the macro there is nothing to assert and the extern
    // would not resolve.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi", feature = "macros"))]
    #[test]
    fn node_macro_emits_dispatch_strategy_symbol() {
        // Re-declare the ABI export the macro just emitted. If the macro
        // elides the symbol (or renames it) this fails to link — exactly
        // the regression the test is meant to catch.
        unsafe extern "C" {
            fn __nros_node_nros_dispatch_strategy() -> u8;
        }
        let strategy = unsafe { __nros_node_nros_dispatch_strategy() };
        // The probe Node uses the default `DISPATCH = Inline`
        // (discriminant 0) — confirms the macro is splicing
        // `<Type as Component>::DISPATCH as u8`, not a hard-coded zero.
        assert_eq!(strategy, crate::DispatchStrategy::Inline as u8);
        assert_eq!(strategy, 0);
    }

    // The `nros::node!()` macro also emits
    // `__nros_node_<pkg>_on_callback`, the extern "C" trampoline the
    // RTIC / Embassy dispatch tasks call after dequeuing a
    // `SignaledCallback<'static>` (see `nros-platform::SignaledCallback`).
    // The expansion lives in the same `dispatch_probe_macro_test`
    // sub-module as the dispatch-strategy probe, so a single
    // `nros_macros::node!(DispatchProbe);` invocation covers both
    // symbols. Symbol name resolves to
    // `__nros_node_nros_on_callback` (CARGO_PKG_NAME = "nros").
    //
    // The test only confirms the symbol is linkable — actually
    // invoking the trampoline would need a live State + CallbackCtx
    // pointer pair, which is the dispatch-task author's contract
    // (documented in the macro emit). A link-only probe is enough to
    // catch the macro silently eliding the export — the exact
    // regression class this test is for.
    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    #[test]
    fn node_macro_emits_on_callback_symbol() {
        unsafe extern "C" {
            fn __nros_node_nros_on_callback(
                state: *mut core::ffi::c_void,
                cb_id_ptr: *const u8,
                cb_id_len: usize,
                ctx: *mut core::ffi::c_void,
            );
        }
        // Take the address of the symbol and feed it through
        // `core::hint::black_box` — forces the linker to resolve the
        // symbol and prevents the optimiser from folding the unused
        // reference away. If the macro stopped emitting the export
        // this line fails at link time, which is the exact regression
        // class this test catches. (`fn`-pointer values are never
        // null per Rust's type system, so a direct null check would
        // be a tautology — `-D useless-ptr-null-checks` would reject
        // it.)
        let fn_ptr: unsafe extern "C" fn(
            *mut core::ffi::c_void,
            *const u8,
            usize,
            *mut core::ffi::c_void,
        ) = __nros_node_nros_on_callback;
        core::hint::black_box(fn_ptr);
    }
}
