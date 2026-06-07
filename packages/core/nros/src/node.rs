//! Rust component API shared by metadata discovery and generated runtimes.

use core::marker::PhantomData;

use crate::{
    ActionTag, CallbackId, CancelResponse, EntityId, GoalId, GoalResponse, GoalStatus,
    ParameterType, QosSettings, RosAction, RosMessage, RosService, ServiceTag, SubscriptionTag,
    TimerDuration,
    heapless::Vec,
    node_metadata::{
        CallbackEffectKind, CallbackEffectMetadata, EntityKind, EntityMetadata, EntityMetadataSpec,
        MetadataRecorder, MetadataString, NodeId, NodeMetadataError, ParameterDefault,
        SourceLocationMetadata, copy_str, entity_metadata,
    },
};

// Phase 212.N.7 step-6 closing sweep — `component_register_symbol`
// removed. It built the legacy `__nros_component_<pkg>_register`
// symbol name for the M.5.a BSP baker to look up by literal. step-6
// retired the macro emit + step-4 deleted the FreeRTOS BSP baker
// crate that was the sole live consumer. The Phase 212.N Entry pkg
// path calls `<pkg>::register(runtime)` through the path API, so this
// helper has no live callers.

/// Clear diagnostic for packages missing [`nros::node!`](crate::node!).
pub const MISSING_NODE_EXPORT_ERROR: &str = "package has no exported nros component";

/// Result type for component declarations.
pub type NodeResult<T = ()> = Result<T, NodeDeclError>;

/// Node declaration error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeDeclError {
    /// Metadata recorder rejected the declaration.
    Metadata(NodeMetadataError),
    /// Host/runtime discovery could not find `nros::node!` export.
    MissingExport,
    /// Generated runtime rejected the declaration.
    Runtime,
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
            Self::MissingExport => MISSING_NODE_EXPORT_ERROR,
            Self::Runtime => "component runtime rejected declaration",
        }
    }
}

impl From<NodeMetadataError> for NodeDeclError {
    fn from(value: NodeMetadataError) -> Self {
        Self::Metadata(value)
    }
}

/// Rust component entry point.
pub trait Node {
    /// Source component name used in metadata and diagnostics.
    const NAME: &'static str;

    /// Phase 216.A.3 — declares which dispatch strategy this Node
    /// requires from the runtime. Defaults to
    /// [`crate::DispatchStrategy::Inline`] so every existing component
    /// keeps compiling without source change; the substrate (Phase
    /// 216.A.2) and `nros check` (Phase 216.D.1) consume it to
    /// pick / validate the board-side dispatch path.
    const DISPATCH: crate::DispatchStrategy = crate::DispatchStrategy::Inline;

    /// Declare nodes, entities, callbacks, params, and optional effects.
    fn register(context: &mut NodeContext<'_>) -> NodeResult<()>;
}

/// Runtime-neutral node construction options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeOptions<'a> {
    /// Source node name. Launch planning may remap/namespace later.
    pub name: &'a str,
    /// Source namespace. Defaults to `/`.
    pub namespace: &'a str,
    /// ROS domain ID hint. Defaults to `0`.
    pub domain_id: u32,
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

    fn create_entity(&mut self, metadata: EntityMetadata) -> NodeResult<()> {
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

/// Runtime node sink used by generated component executors.
///
/// Metadata mode records declarations only. Runtime mode maps each stable
/// component node ID to a concrete executor-side node handle; entity callback
/// registration is completed by generated code that owns the actual callback
/// functions.
pub trait DeclaredNodeRuntime {
    /// Concrete node handle owned by the runtime executor.
    type NodeHandle: Copy + Eq;

    /// Create a runtime node from source-level component options.
    fn build_component_node(
        &mut self,
        id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> NodeResult<Self::NodeHandle>;
}

/// Recorded runtime node mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeNodeRecord<H: Copy + Eq> {
    stable_id: MetadataString,
    handle: H,
}

impl<H: Copy + Eq> RuntimeNodeRecord<H> {
    /// Stable component node ID.
    pub fn stable_id(&self) -> &str {
        &self.stable_id
    }

    /// Runtime executor node handle.
    pub const fn handle(&self) -> H {
        self.handle
    }
}

/// Runtime adapter used by generated main ownership code.
pub struct NodeRuntimeAdapter<
    'a,
    R: DeclaredNodeRuntime + ?Sized,
    const MAX_NODES: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_NODES },
    const MAX_ENTITIES: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_ENTITIES },
    const MAX_CALLBACKS: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_CALLBACKS },
> {
    node_runtime: &'a mut R,
    nodes: Vec<RuntimeNodeRecord<R::NodeHandle>, MAX_NODES>,
    entities: Vec<EntityMetadata, MAX_ENTITIES>,
    callback_effects: Vec<CallbackEffectMetadata, MAX_CALLBACKS>,
}

impl<
    'a,
    R: DeclaredNodeRuntime + ?Sized,
    const MAX_NODES: usize,
    const MAX_ENTITIES: usize,
    const MAX_CALLBACKS: usize,
> NodeRuntimeAdapter<'a, R, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    /// Build a runtime adapter around a generated executor owner.
    pub fn new(node_runtime: &'a mut R) -> Self {
        Self {
            node_runtime,
            nodes: Vec::new(),
            entities: Vec::new(),
            callback_effects: Vec::new(),
        }
    }

    /// Runtime node mappings in declaration order.
    pub fn nodes(&self) -> &[RuntimeNodeRecord<R::NodeHandle>] {
        &self.nodes
    }

    /// Entity declarations accepted for generated runtime binding.
    pub fn entities(&self) -> &[EntityMetadata] {
        &self.entities
    }

    /// Optional callback effects accepted for generated runtime binding.
    pub fn callback_effects(&self) -> &[CallbackEffectMetadata] {
        &self.callback_effects
    }

    /// Lookup an executor node handle by stable component node ID.
    pub fn node_handle(&self, stable_id: NodeId<'_>) -> Option<R::NodeHandle> {
        self.nodes
            .iter()
            .find(|node| node.stable_id() == stable_id.as_str())
            .map(RuntimeNodeRecord::handle)
    }

    fn contains_node(&self, stable_id: &str) -> bool {
        self.nodes.iter().any(|node| node.stable_id() == stable_id)
    }

    fn contains_entity(&self, stable_id: &str) -> bool {
        self.entities
            .iter()
            .any(|entity| entity.id.as_str() == stable_id)
    }
}

impl<
    R: DeclaredNodeRuntime + ?Sized,
    const MAX_NODES: usize,
    const MAX_ENTITIES: usize,
    const MAX_CALLBACKS: usize,
> NodeRuntime for NodeRuntimeAdapter<'_, R, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> NodeResult<()> {
        if self.contains_node(id.as_str()) {
            return Err(NodeMetadataError::DuplicateId.into());
        }
        let handle = self.node_runtime.build_component_node(id, options)?;
        self.nodes
            .push(RuntimeNodeRecord {
                stable_id: copy_str(id.as_str())?,
                handle,
            })
            .map_err(|_| NodeDeclError::Metadata(NodeMetadataError::Capacity))?;
        Ok(())
    }

    fn create_entity(&mut self, metadata: EntityMetadata) -> NodeResult<()> {
        if !self.contains_node(metadata.node_id.as_str()) {
            return Err(NodeMetadataError::UnknownNode.into());
        }
        if self.contains_entity(metadata.id.as_str()) {
            return Err(NodeMetadataError::DuplicateId.into());
        }
        self.entities
            .push(metadata)
            .map_err(|_| NodeDeclError::Metadata(NodeMetadataError::Capacity))?;
        Ok(())
    }

    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> NodeResult<()> {
        if !self.contains_entity(entity_id.as_str()) {
            return Err(NodeMetadataError::UnknownEntity.into());
        }
        self.callback_effects
            .push(CallbackEffectMetadata {
                callback_id: copy_str(callback_id.as_str())?,
                kind,
                entity_id: copy_str(entity_id.as_str())?,
            })
            .map_err(|_| NodeDeclError::Metadata(NodeMetadataError::Capacity))?;
        Ok(())
    }
}

#[cfg(feature = "rmw-cffi")]
impl DeclaredNodeRuntime for crate::Executor {
    type NodeHandle = nros_node::executor::NodeId;

    fn build_component_node(
        &mut self,
        _id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> NodeResult<Self::NodeHandle> {
        self.node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| NodeDeclError::Runtime)
    }
}

/// Runtime adapter backed by [`Executor`](crate::Executor).
#[cfg(feature = "rmw-cffi")]
pub type NodeExecutorRuntime<
    'a,
    const MAX_NODES: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_NODES },
    const MAX_ENTITIES: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_ENTITIES },
    const MAX_CALLBACKS: usize = { crate::node_metadata::DEFAULT_MAX_METADATA_CALLBACKS },
> = NodeRuntimeAdapter<'a, crate::Executor, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>;

/// Node declaration context. Does not own middleware transport.
pub struct NodeContext<'a, R: NodeRuntime + ?Sized = dyn NodeRuntime + 'a> {
    component_name: &'static str,
    runtime: &'a mut R,
}

impl<'a, R: NodeRuntime + ?Sized> NodeContext<'a, R> {
    /// Build a context over a metadata recorder or generated runtime.
    pub fn new(component_name: &'static str, runtime: &'a mut R) -> Self {
        Self {
            component_name,
            runtime,
        }
    }

    /// Source component name.
    pub const fn component_name(&self) -> &'static str {
        self.component_name
    }

    /// Declare a node with an explicit stable node ID.
    pub fn create_node<'id>(
        &mut self,
        id: NodeId<'id>,
        options: NodeOptions<'_>,
    ) -> NodeResult<DeclaredNode<'_, 'id, R>> {
        self.create_node_with_id(id, options)
    }

    /// Declare a node with an explicit stable node ID.
    ///
    /// This is the advanced form for packages that declare multiple nodes or
    /// need stable metadata IDs that differ from their ROS node names. Prefer
    /// [`create_node_with_options`](Self::create_node_with_options) for common
    /// one-node packages.
    pub fn create_node_with_id<'id>(
        &mut self,
        id: NodeId<'id>,
        options: NodeOptions<'_>,
    ) -> NodeResult<DeclaredNode<'_, 'id, R>> {
        self.runtime.create_node(id, options)?;
        Ok(DeclaredNode {
            runtime: self.runtime,
            id,
        })
    }

    /// Declare a node using `options.name` as the stable node ID.
    ///
    /// This mirrors the common rclcpp/rclrs shape where a node package supplies
    /// node options and the node name, while nano-ros keeps the stable ID as
    /// internal metadata. Packages with more than one node should use
    /// [`create_node_with_id`](Self::create_node_with_id) when names are not
    /// sufficient stable IDs.
    pub fn create_node_with_options<'id>(
        &mut self,
        options: NodeOptions<'id>,
    ) -> NodeResult<DeclaredNode<'_, 'id, R>> {
        self.create_node_with_id(NodeId::new(options.name), options)
    }

    /// Record optional effects for a callback not tied to a node wrapper.
    pub fn callback<'id>(&mut self, id: CallbackId<'id>) -> CallbackEffects<'_, 'id, R> {
        CallbackEffects {
            runtime: self.runtime,
            id,
        }
    }
}

/// Declared component node.
pub struct DeclaredNode<'ctx, 'id, R: NodeRuntime + ?Sized = dyn NodeRuntime + 'ctx> {
    runtime: &'ctx mut R,
    id: NodeId<'id>,
}

impl<'ctx, 'id, R: NodeRuntime + ?Sized> DeclaredNode<'ctx, 'id, R> {
    /// Stable node ID.
    pub const fn id(&self) -> NodeId<'id> {
        self.id
    }

    /// Declare a publisher with default QoS. Stable publisher ID is required.
    #[track_caller]
    pub fn create_publisher<'entity, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.create_publisher_with_qos::<M>(id, topic, QosSettings::default())
    }

    /// Declare a publisher using `topic` as the stable entity ID.
    ///
    /// Use the explicit [`create_publisher`](Self::create_publisher) form when
    /// a node declares more than one publisher on the same topic or needs a
    /// stable metadata ID that differs from the ROS topic name.
    #[track_caller]
    pub fn create_publisher_for_topic<'entity, M: RosMessage>(
        &mut self,
        topic: &'entity str,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.create_publisher_for_topic_with_qos::<M>(topic, QosSettings::default())
    }

    /// Declare a publisher with explicit QoS, using `topic` as the stable entity ID.
    #[track_caller]
    pub fn create_publisher_for_topic_with_qos<'entity, M: RosMessage>(
        &mut self,
        topic: &'entity str,
        qos: QosSettings,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        self.create_publisher_with_qos::<M>(EntityId::new(topic), topic, qos)
    }

    /// Declare a publisher with explicit QoS.
    #[track_caller]
    pub fn create_publisher_with_qos<'entity, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
        qos: QosSettings,
    ) -> NodeResult<NodePublisher<'entity, M>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::Publisher,
            source_name: topic,
            type_name: M::TYPE_NAME,
            type_hash: M::TYPE_HASH,
            qos,
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.runtime.create_entity(metadata)?;
        Ok(NodePublisher::new(id))
    }

    /// Declare a subscription. Stable subscription and callback IDs are required.
    #[track_caller]
    pub fn create_subscription<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.create_subscription_with_qos::<M>(id, callback_id, topic, QosSettings::default())
    }

    /// Declare a subscription using `callback_id` as the stable entity ID.
    ///
    /// This mirrors the common single-callback case: the callback name is the
    /// source-level stable ID, while `topic` remains the ROS topic name.
    #[track_caller]
    pub fn create_subscription_for_callback<'callback, M: RosMessage>(
        &mut self,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        self.create_subscription_for_callback_with_qos::<M>(
            callback_id,
            topic,
            QosSettings::default(),
        )
    }

    /// Declare a subscription with explicit QoS, using `callback_id` as the stable entity ID.
    #[track_caller]
    pub fn create_subscription_for_callback_with_qos<'callback, M: RosMessage>(
        &mut self,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QosSettings,
    ) -> NodeResult<NodeSubscription<'callback, M>> {
        self.create_subscription_with_qos::<M>(
            EntityId::new(callback_id.as_str()),
            callback_id,
            topic,
            qos,
        )
    }

    /// Declare a subscription using `topic` as both the stable entity ID and callback ID.
    #[track_caller]
    pub fn create_subscription_for_topic<'entity, M: RosMessage>(
        &mut self,
        topic: &'entity str,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.create_subscription_for_topic_with_qos::<M>(topic, QosSettings::default())
    }

    /// Declare a subscription with explicit QoS, using `topic` as both IDs.
    #[track_caller]
    pub fn create_subscription_for_topic_with_qos<'entity, M: RosMessage>(
        &mut self,
        topic: &'entity str,
        qos: QosSettings,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        self.create_subscription_with_qos::<M>(
            EntityId::new(topic),
            CallbackId::new(topic),
            topic,
            qos,
        )
    }

    /// Declare a subscription with explicit QoS.
    #[track_caller]
    pub fn create_subscription_with_qos<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QosSettings,
    ) -> NodeResult<NodeSubscription<'entity, M>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::Subscription,
            source_name: topic,
            type_name: M::TYPE_NAME,
            type_hash: M::TYPE_HASH,
            qos,
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(NodeSubscription::new(id))
    }

    /// Declare a subscription whose stable entity and callback IDs are
    /// both synthesized from the topic literal, returning a
    /// [`SubscriptionTag`] the Node author stores on `Self::State` and
    /// matches against the `&CallbackId<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// Use this on the Phase 216.A Deferred Node path where the Node
    /// author does not need to invent a separate stable entity ID — the
    /// topic literal becomes both the entity ID and the callback ID,
    /// and the returned tag preserves that identifier for compile-time
    /// `state.sub_chatter == cb` matches in `on_callback`.
    #[track_caller]
    pub fn create_subscription_static<M: RosMessage>(
        &mut self,
        topic: &'static str,
    ) -> NodeResult<SubscriptionTag> {
        let id = EntityId::new(topic);
        let callback_id = CallbackId::new(topic);
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::Subscription,
            source_name: topic,
            type_name: M::TYPE_NAME,
            type_hash: M::TYPE_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(SubscriptionTag::new(topic))
    }

    /// Declare a timer. Stable timer and callback IDs are required.
    #[track_caller]
    pub fn create_timer<'entity, 'callback>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> NodeResult<NodeTimer<'entity>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::Timer,
            source_name: "",
            type_name: "",
            type_hash: "",
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        metadata.period_ms = Some(period.as_millis());
        self.runtime.create_entity(metadata)?;
        Ok(NodeTimer::new(id))
    }

    /// Declare a timer using `callback_id` as the stable timer entity ID.
    #[track_caller]
    pub fn create_timer_for_callback<'callback>(
        &mut self,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> NodeResult<NodeTimer<'callback>> {
        self.create_timer(EntityId::new(callback_id.as_str()), callback_id, period)
    }

    /// Declare a service server. Stable service and callback IDs are required.
    #[track_caller]
    pub fn create_service_server<'entity, 'callback, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        service_name: &str,
    ) -> NodeResult<NodeServiceServer<'entity, S>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ServiceServer,
            source_name: service_name,
            type_name: S::SERVICE_NAME,
            type_hash: S::SERVICE_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(NodeServiceServer::new(id))
    }

    /// Declare a service server whose stable entity and callback IDs are
    /// both synthesized from the service-name literal, returning a
    /// [`ServiceTag`] the Node author stores on `Self::State` and matches
    /// against the `&CallbackId<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// Tag-only registration is restricted to the SERVER side: clients
    /// need a USABLE handle (`NodeServiceClient`) to issue requests, so
    /// use the existing
    /// [`create_service_client`](Self::create_service_client) builder
    /// for the client side.
    #[track_caller]
    pub fn create_service_static<S: RosService>(
        &mut self,
        name: &'static str,
    ) -> NodeResult<ServiceTag> {
        let id = EntityId::new(name);
        let callback_id = CallbackId::new(name);
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ServiceServer,
            source_name: name,
            type_name: S::SERVICE_NAME,
            type_hash: S::SERVICE_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(ServiceTag::new(name))
    }

    /// Declare a service client. Stable service client ID is required.
    #[track_caller]
    pub fn create_service_client<'entity, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        service_name: &str,
    ) -> NodeResult<NodeServiceClient<'entity, S>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ServiceClient,
            source_name: service_name,
            type_name: S::SERVICE_NAME,
            type_hash: S::SERVICE_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.runtime.create_entity(metadata)?;
        Ok(NodeServiceClient::new(id))
    }

    /// Declare an action server. Stable action and callback IDs are required.
    #[track_caller]
    pub fn create_action_server<'entity, 'callback, A: RosAction>(
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
    pub fn create_action_server_with_callbacks<'entity, 'goal, 'cancel, 'accepted, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        goal_callback_id: CallbackId<'goal>,
        cancel_callback_id: CallbackId<'cancel>,
        accepted_callback_id: CallbackId<'accepted>,
        action_name: &str,
    ) -> NodeResult<NodeActionServer<'entity, A>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ActionServer,
            source_name: action_name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(goal_callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.action_cancel_callback_id = Some(copy_str(cancel_callback_id.as_str())?);
        metadata.action_cancel_source = metadata.callback_source.clone();
        metadata.action_accepted_callback_id = Some(copy_str(accepted_callback_id.as_str())?);
        metadata.action_accepted_source = metadata.callback_source.clone();
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(NodeActionServer::new(id))
    }

    /// Declare an action server whose stable entity and callback IDs are
    /// both synthesized from the action-name literal, returning an
    /// [`ActionTag`] the Node author stores on `Self::State` and matches
    /// against the `&CallbackId<'_>` delivered to
    /// [`ExecutableNode::on_callback`].
    ///
    /// The synthesized callback ID is shared by the goal / cancel /
    /// accepted callbacks (matching the default behavior of
    /// [`create_action_server`](Self::create_action_server)).
    ///
    /// Tag-only registration is restricted to the SERVER side: clients
    /// need a USABLE handle (`NodeActionClient`) to dispatch goals, so
    /// use the existing
    /// [`create_action_client`](Self::create_action_client) builder for
    /// the client side.
    #[track_caller]
    pub fn create_action_static<A: RosAction>(
        &mut self,
        name: &'static str,
    ) -> NodeResult<ActionTag> {
        let id = EntityId::new(name);
        let callback_id = CallbackId::new(name);
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ActionServer,
            source_name: name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.callback_source = SourceLocationMetadata::caller()?;
        metadata.action_cancel_callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.action_cancel_source = metadata.callback_source.clone();
        metadata.action_accepted_callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.action_accepted_source = metadata.callback_source.clone();
        metadata.source = metadata.callback_source.clone();
        self.runtime.create_entity(metadata)?;
        Ok(ActionTag::new(name))
    }

    /// Declare an action client. Stable action client ID is required.
    #[track_caller]
    pub fn create_action_client<'entity, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        action_name: &str,
    ) -> NodeResult<NodeActionClient<'entity, A>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::ActionClient,
            source_name: action_name,
            type_name: A::ACTION_NAME,
            type_hash: A::ACTION_HASH,
            qos: QosSettings::default(),
        })?;
        metadata.source = SourceLocationMetadata::caller()?;
        self.runtime.create_entity(metadata)?;
        Ok(NodeActionClient::new(id))
    }

    /// Declare a parameter. Stable parameter ID is required.
    #[track_caller]
    pub fn declare_parameter<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        parameter_type: ParameterType,
    ) -> NodeResult<NodeParameter<'entity>> {
        self.declare_parameter_with_default(id, name, ParameterDefault::for_type(parameter_type)?)
    }

    /// Declare a parameter with a concrete source default.
    #[track_caller]
    pub fn declare_parameter_with_default<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        default: ParameterDefault,
    ) -> NodeResult<NodeParameter<'entity>> {
        let mut metadata = entity_metadata(EntityMetadataSpec {
            id,
            node_id: self.id,
            kind: EntityKind::Parameter,
            source_name: name,
            type_name: "",
            type_hash: "",
            qos: QosSettings::default(),
        })?;
        metadata.parameter_type = Some(default.parameter_type());
        metadata.parameter_default = Some(default);
        metadata.source = SourceLocationMetadata::caller()?;
        self.runtime.create_entity(metadata)?;
        Ok(NodeParameter::new(id))
    }

    /// Record optional effects for a callback.
    pub fn callback<'callback>(
        &mut self,
        id: CallbackId<'callback>,
    ) -> CallbackEffects<'_, 'callback, R> {
        CallbackEffects {
            runtime: self.runtime,
            id,
        }
    }
}

/// Builder for optional callback effect metadata.
pub struct CallbackEffects<'ctx, 'id, R: NodeRuntime + ?Sized = dyn NodeRuntime + 'ctx> {
    runtime: &'ctx mut R,
    id: CallbackId<'id>,
}

impl<'ctx, 'id, R: NodeRuntime + ?Sized> CallbackEffects<'ctx, 'id, R> {
    /// Record that callback reads from an entity.
    pub fn reads(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Reads, entity_id)?;
        Ok(self)
    }

    /// Record that callback reads from a declared entity handle.
    pub fn reads_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.reads(entity.entity_id())
    }

    /// Record that callback publishes via an entity.
    pub fn publishes(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Publishes, entity_id)?;
        Ok(self)
    }

    /// Record that callback publishes via a declared entity handle.
    pub fn publishes_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.publishes(entity.entity_id())
    }

    /// Record that callback writes to an entity or parameter.
    pub fn writes(self, entity_id: EntityId<'_>) -> NodeResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Writes, entity_id)?;
        Ok(self)
    }

    /// Record that callback writes to a declared entity handle.
    pub fn writes_entity(self, entity: &impl DeclaredEntity) -> NodeResult<Self> {
        self.writes(entity.entity_id())
    }
}

/// A declared source-level entity handle that can be referenced by callback effects.
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
pub struct CallbackCtx<'a> {
    payload: &'a [u8],
    publishers: &'a dyn PublisherResolver,
    reply: Option<ReplySink<'a>>,
    decision: Option<DecisionSink<'a>>,
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
        }
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

    /// Deserialize the triggering payload as `M` (subscription / service-request
    /// bodies). `Err` if the payload is malformed for `M`.
    pub fn message<M: RosMessage>(&self) -> NodeResult<M> {
        let mut reader =
            crate::CdrReader::new_with_header(self.payload).map_err(|_| NodeDeclError::Runtime)?;
        M::deserialize(&mut reader).map_err(|_| NodeDeclError::Runtime)
    }

    /// Publish raw CDR bytes through the named publisher entity (immediate).
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> NodeResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
    /// `N` must be ≥ the CDR-encoded size of `msg`; the generated runtime picks
    /// it from the message type.
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
    /// [`DeclaredNode::create_publisher_for_topic`], allowing simple callback
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

/// The executable counterpart of [`Node`] (W.5.1).
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
    /// The synchronous block model matches the existing
    /// `ServiceClientTrait::call_raw` surface in nros-node — the tick
    /// hook drives the executor between callback dispatch, so a blocked
    /// `call_raw` does not starve other callbacks (each tick yields back
    /// to the runtime after returning).
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
        }
    }

    /// Publish raw CDR bytes through the named publisher entity (immediate).
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> NodeResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
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
    /// This pairs with [`DeclaredNode::create_publisher_for_topic`] for
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
    pub fn complete_goal<R: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &R,
    ) -> NodeResult<()> {
        // Header-less inner CDR: the executor's `complete_goal_raw` frames the
        // outer envelope (matches the typed `ActionServerHandle::complete_goal`).
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        result
            .serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.actions
            .complete_goal_raw(action.as_str(), goal_id, status, &buf[..len])
    }

    /// Visit each active (accepted, not yet completed) goal on `action` with its
    /// id + status — how a `tick` body discovers goals to feed / complete. Collect
    /// the ids you want to act on, then call [`Self::publish_feedback`] /
    /// [`Self::complete_goal`] after the visit returns (those borrow `self`
    /// mutably, so they can't run inside `visit`).
    pub fn for_each_active_goal(
        &self,
        action: EntityId<'_>,
        visit: &mut dyn FnMut(&GoalId, GoalStatus),
    ) {
        self.actions.for_each_active_goal(action.as_str(), visit);
    }

    /// Publish typed feedback for an active action goal (W.5.6 — tick-only).
    pub fn publish_feedback<F: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal_id: &GoalId,
        feedback: &F,
    ) -> NodeResult<()> {
        // Header-less inner CDR: the executor's `publish_feedback_raw` frames the
        // outer envelope (matches the typed `ActionServerHandle::publish_feedback`).
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        feedback
            .serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.actions
            .publish_feedback_raw(action.as_str(), goal_id, &buf[..len])
    }

    /// Issue a service-client raw-CDR request and block on the reply
    /// (M-F.4 — tick-only). Writes the response CDR into `response_buf`
    /// and returns the response length in bytes.
    pub fn call_raw(
        &mut self,
        service: EntityId<'_>,
        request_cdr: &[u8],
        response_buf: &mut [u8],
    ) -> NodeResult<usize> {
        self.clients
            .call_raw(service.as_str(), request_cdr, response_buf)
    }

    /// Issue a typed service-client request and decode the reply
    /// (M-F.4 — tick-only). `REQ_N` / `RESP_N` stack-size the request /
    /// response CDR buffers; size them via
    /// `<<Req as RosMessage>::SerializedSize as nros::SerializedSize>::SIZE`.
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

    /// Send a raw-CDR action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). Result + feedback streams arrive
    /// via callback dispatch; this method only kicks off the request.
    pub fn send_goal_raw(&mut self, action: EntityId<'_>, goal_cdr: &[u8]) -> NodeResult<GoalId> {
        self.clients.send_goal_raw(action.as_str(), goal_cdr)
    }

    /// Send a typed action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). `N` stack-sizes the goal CDR
    /// buffer.
    pub fn send_goal<G: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal: &G,
    ) -> NodeResult<GoalId> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| NodeDeclError::Runtime)?;
        goal.serialize(&mut writer)
            .map_err(|_| NodeDeclError::Runtime)?;
        let len = writer.position();
        self.clients.send_goal_raw(action.as_str(), &buf[..len])
    }
}

pub trait ExecutableNode: Node {
    /// Per-instance mutable state shared across the component's callbacks.
    type State;

    /// Build the initial state (called once by the generated runtime).
    fn init() -> Self::State;

    /// Run the body for `callback`. `ctx` exposes the triggering payload + the
    /// immediate publish path. Bodies match on `callback` (the stable id from
    /// the declarative `create_*` calls).
    fn on_callback(state: &mut Self::State, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>);

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
/// impl nros::Node for Node { /* register(...) */ }
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
                _callback: $crate::CallbackId<'_>,
                _ctx: &mut $crate::CallbackCtx<'_>,
            ) {
            }
        }
    };
}

/// Run component registration against any component runtime.
pub fn register_node<C: Node>(runtime: &mut dyn NodeRuntime) -> NodeResult<()> {
    let mut context = NodeContext::new(C::NAME, runtime);
    C::register(&mut context)
}

/// Phase 212.M.5.a.4 internal — `Box`-erase a freshly built component
/// `State` to the type-erased `*mut ()` ABI the BSP path uses. Called
/// only from the `nros::node!()` macro emit; not public API.
///
/// The returned pointer is a leaked `Box`; the BSP runtime keeps it
/// alive for the firmware lifetime (embedded slots never deallocate).
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub fn __private_node_state_into_raw<C: ExecutableNode>(state: C::State) -> *mut () {
    extern crate alloc;
    alloc::boxed::Box::into_raw(alloc::boxed::Box::new(state)) as *mut ()
}

/// Run component registration against an in-memory metadata recorder.
pub fn record_node_metadata<C: Node>(recorder: &mut dyn NodeRuntime) -> NodeResult<()> {
    register_node::<C>(recorder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CdrReader, CdrWriter, DeserError, SerError, SourceNameKind};

    #[derive(Default)]
    struct FakeNodeRuntime {
        next: u8,
        created: Vec<MetadataString, 4>,
    }

    impl DeclaredNodeRuntime for FakeNodeRuntime {
        type NodeHandle = u8;

        fn build_component_node(
            &mut self,
            _id: NodeId<'_>,
            options: NodeOptions<'_>,
        ) -> NodeResult<Self::NodeHandle> {
            self.created
                .push(copy_str(options.name)?)
                .map_err(|_| NodeDeclError::Metadata(NodeMetadataError::Capacity))?;
            let handle = self.next;
            self.next += 1;
            Ok(handle)
        }
    }

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

    struct TestService;

    impl RosService for TestService {
        type Request = TestMsg;
        type Reply = TestMsg;

        const SERVICE_NAME: &'static str = "test_msgs::srv::dds_::Test_";
        const SERVICE_HASH: &'static str = "test_service_hash";
    }

    struct TestAction;

    impl RosAction for TestAction {
        type Goal = TestMsg;
        type Result = TestMsg;
        type Feedback = TestMsg;
        type SendGoalRequest = TestMsg;
        type SendGoalResponse = TestMsg;
        type GetResultRequest = TestMsg;
        type GetResultResponse = TestMsg;
        type FeedbackMessage = TestMsg;

        const ACTION_NAME: &'static str = "test_msgs::action::dds_::Test_";
        const ACTION_HASH: &'static str = "test_action_hash";
    }

    struct TalkerComponent;

    impl Node for TalkerComponent {
        const NAME: &'static str = "talker_component";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            let mut node = context.create_node(NodeId::new("node"), NodeOptions::new("talker"))?;
            let _publisher =
                node.create_publisher::<TestMsg>(EntityId::new("pub_chatter"), "chatter")?;
            let _subscription = node.create_subscription::<TestMsg>(
                EntityId::new("sub_cmd"),
                CallbackId::new("on_cmd"),
                "~/cmd",
            )?;
            let _timer = node.create_timer(
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
    fn component_records_metadata_without_transport() {
        let mut recorder = MetadataRecorder::<2, 8, 4>::new();
        record_node_metadata::<TalkerComponent>(&mut recorder).unwrap();

        assert_eq!(recorder.nodes().len(), 1);
        assert_eq!(recorder.nodes()[0].name.as_str(), "talker");
        assert_eq!(recorder.entities().len(), 4);
        assert_eq!(recorder.entities()[0].kind, EntityKind::Publisher);
        assert_eq!(recorder.entities()[1].source_name.as_str(), "~/cmd");
        assert_eq!(
            recorder.entities()[1]
                .callback_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("on_cmd")
        );
        assert_eq!(recorder.callback_effects().len(), 2);
    }

    #[test]
    fn runtime_adapter_maps_stable_nodes_to_runtime_handles() {
        let mut node_runtime = FakeNodeRuntime::default();
        let mut runtime = NodeRuntimeAdapter::<_, 2, 8, 4>::new(&mut node_runtime);

        register_node::<TalkerComponent>(&mut runtime).unwrap();

        assert_eq!(runtime.nodes().len(), 1);
        assert_eq!(runtime.nodes()[0].stable_id(), "node");
        assert_eq!(runtime.node_handle(NodeId::new("node")), Some(0));
        assert_eq!(runtime.entities().len(), 4);
        assert_eq!(runtime.callback_effects().len(), 2);
    }

    #[test]
    fn context_can_synthesize_stable_node_id_from_options_name() {
        let mut recorder = MetadataRecorder::<1, 0, 0>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let node = context
            .create_node_with_options(NodeOptions::new("talker").namespace("/demo").domain_id(42))
            .unwrap();

        assert_eq!(node.id(), NodeId::new("talker"));
        drop(node);
        drop(context);
        assert_eq!(recorder.nodes().len(), 1);
        assert_eq!(recorder.nodes()[0].id.as_str(), "talker");
        assert_eq!(recorder.nodes()[0].name.as_str(), "talker");
        assert_eq!(recorder.nodes()[0].namespace.as_str(), "/demo");
        assert_eq!(recorder.nodes()[0].domain_id, 42);
    }

    #[test]
    fn synthesized_node_ids_reject_duplicate_names() {
        let mut node_runtime = FakeNodeRuntime::default();
        let mut runtime = NodeRuntimeAdapter::<_, 2, 0, 0>::new(&mut node_runtime);
        {
            let mut context = NodeContext::new("test", &mut runtime);
            context
                .create_node_with_options(NodeOptions::new("talker"))
                .unwrap();
        }
        let mut context = NodeContext::new("test", &mut runtime);
        let result = context.create_node_with_options(NodeOptions::new("talker"));

        assert!(matches!(
            result,
            Err(NodeDeclError::Metadata(NodeMetadataError::DuplicateId))
        ));
    }

    #[test]
    fn synthesized_entity_helpers_record_topic_and_callback_ids() {
        let mut recorder = MetadataRecorder::<1, 3, 2>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let mut node = context
            .create_node_with_options(NodeOptions::new("talker"))
            .unwrap();

        let publisher = node
            .create_publisher_for_topic::<TestMsg>("/chatter")
            .unwrap();
        let subscription = node
            .create_subscription_for_callback::<TestMsg>(CallbackId::new("on_message"), "/cmd")
            .unwrap();
        let _timer = node
            .create_timer_for_callback(CallbackId::new("on_tick"), TimerDuration::from_millis(10))
            .unwrap();

        node.callback(CallbackId::new("on_tick"))
            .publishes_entity(&publisher)
            .unwrap();
        node.callback(CallbackId::new("on_message"))
            .reads_entity(&subscription)
            .unwrap();

        assert_eq!(publisher.id(), EntityId::new("/chatter"));
        assert_eq!(subscription.id(), EntityId::new("on_message"));
        assert_eq!(recorder.entities().len(), 3);

        let publisher = &recorder.entities()[0];
        assert_eq!(publisher.id.as_str(), "/chatter");
        assert_eq!(publisher.kind, EntityKind::Publisher);
        assert_eq!(publisher.source_name.as_str(), "/chatter");

        let subscription = &recorder.entities()[1];
        assert_eq!(subscription.id.as_str(), "on_message");
        assert_eq!(subscription.kind, EntityKind::Subscription);
        assert_eq!(subscription.source_name.as_str(), "/cmd");
        assert_eq!(
            subscription.callback_id.as_ref().map(|id| id.as_str()),
            Some("on_message")
        );

        let timer = &recorder.entities()[2];
        assert_eq!(timer.id.as_str(), "on_tick");
        assert_eq!(timer.kind, EntityKind::Timer);
        assert_eq!(
            timer.callback_id.as_ref().map(|id| id.as_str()),
            Some("on_tick")
        );

        assert_eq!(recorder.callback_effects().len(), 2);
        assert_eq!(
            recorder.callback_effects()[0].entity_id.as_str(),
            "/chatter"
        );
        assert_eq!(
            recorder.callback_effects()[1].entity_id.as_str(),
            "on_message"
        );
    }

    #[test]
    fn synthesized_entity_ids_reject_collisions() {
        let mut recorder = MetadataRecorder::<1, 2, 0>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let mut node = context
            .create_node_with_options(NodeOptions::new("talker"))
            .unwrap();

        node.create_publisher_for_topic::<TestMsg>("/chatter")
            .unwrap();
        let result = node.create_publisher_for_topic::<TestMsg>("/chatter");

        assert!(matches!(
            result,
            Err(NodeDeclError::Metadata(NodeMetadataError::DuplicateId))
        ));
    }

    /// Verifies the runtime adapter rejects duplicate nodes and unknown effect entities.
    #[test]
    fn runtime_adapter_rejects_unknown_entities() {
        let mut node_runtime = FakeNodeRuntime::default();
        let mut runtime = NodeRuntimeAdapter::<_, 1, 1, 1>::new(&mut node_runtime);
        runtime
            .create_node(NodeId::new("node"), NodeOptions::new("talker"))
            .unwrap();

        assert_eq!(
            runtime.create_node(NodeId::new("node"), NodeOptions::new("other")),
            Err(NodeDeclError::Metadata(NodeMetadataError::DuplicateId))
        );
        assert_eq!(
            runtime.record_callback_effect(
                CallbackId::new("cb"),
                CallbackEffectKind::Reads,
                EntityId::new("missing")
            ),
            Err(NodeDeclError::Metadata(NodeMetadataError::UnknownEntity))
        );
    }

    #[test]
    fn component_rejects_effect_for_unknown_entity() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let result = context
            .callback(CallbackId::new("cb"))
            .reads(EntityId::new("missing"));
        assert!(matches!(
            result,
            Err(NodeDeclError::Metadata(NodeMetadataError::UnknownEntity))
        ));
    }

    #[test]
    fn component_missing_export_error_message_is_clear() {
        assert_eq!(
            NodeDeclError::MissingExport.message(),
            MISSING_NODE_EXPORT_ERROR
        );
        assert_eq!(
            NodeDeclError::MissingExport.message(),
            "package has no exported nros component"
        );
    }

    struct RobotComponent;

    impl Node for RobotComponent {
        const NAME: &'static str = "robot_component";

        fn register(context: &mut NodeContext<'_>) -> NodeResult<()> {
            {
                let mut sensors = context
                    .create_node(NodeId::new("node_sensors"), NodeOptions::new("sensors"))?;
                let _status =
                    sensors.create_publisher::<TestMsg>(EntityId::new("pub_status"), "~/status")?;
            }

            let mut control =
                context.create_node(NodeId::new("node_control"), NodeOptions::new("control"))?;
            let _cmd = control.create_subscription::<TestMsg>(
                EntityId::new("sub_cmd"),
                CallbackId::new("cb_cmd"),
                "~/cmd",
            )?;
            let _reset = control.create_service_server::<TestService>(
                EntityId::new("srv_reset"),
                CallbackId::new("cb_reset"),
                "reset",
            )?;
            let _navigate = control.create_action_server_with_callbacks::<TestAction>(
                EntityId::new("act_navigate"),
                CallbackId::new("cb_nav_goal"),
                CallbackId::new("cb_nav_cancel"),
                CallbackId::new("cb_nav_accepted"),
                "~/navigate",
            )?;
            let _gain = control.declare_parameter_with_default(
                EntityId::new("param_gain"),
                "gain",
                ParameterDefault::Double(copy_str("1.5")?),
            )?;

            control
                .callback(CallbackId::new("cb_cmd"))
                .publishes(EntityId::new("pub_status"))?
                .reads(EntityId::new("param_gain"))?;
            control
                .callback(CallbackId::new("cb_nav_accepted"))
                .writes(EntityId::new("param_gain"))?;

            Ok(())
        }
    }

    /// Verifies the component API records multi-node services, actions, and defaults.
    #[test]
    fn component_api_records_multi_node_services() {
        let mut recorder = MetadataRecorder::<4, 12, 4>::new();
        record_node_metadata::<RobotComponent>(&mut recorder).unwrap();

        assert_eq!(recorder.nodes().len(), 2);
        assert_eq!(recorder.nodes()[0].id.as_str(), "node_sensors");
        assert_eq!(recorder.nodes()[1].id.as_str(), "node_control");

        let status = recorder
            .entities()
            .iter()
            .find(|entity| entity.id.as_str() == "pub_status")
            .unwrap();
        assert_eq!(status.kind, EntityKind::Publisher);
        assert_eq!(status.source_name.as_str(), "~/status");
        assert_eq!(status.source_name_kind, SourceNameKind::Private);

        let reset = recorder
            .entities()
            .iter()
            .find(|entity| entity.id.as_str() == "srv_reset")
            .unwrap();
        assert_eq!(reset.kind, EntityKind::ServiceServer);
        assert_eq!(
            reset.callback_id.as_ref().map(|id| id.as_str()),
            Some("cb_reset")
        );

        let navigate = recorder
            .entities()
            .iter()
            .find(|entity| entity.id.as_str() == "act_navigate")
            .unwrap();
        assert_eq!(navigate.kind, EntityKind::ActionServer);
        assert_eq!(
            navigate.callback_id.as_ref().map(|id| id.as_str()),
            Some("cb_nav_goal")
        );
        assert_eq!(
            navigate
                .action_cancel_callback_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("cb_nav_cancel")
        );
        assert_eq!(
            navigate
                .action_accepted_callback_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("cb_nav_accepted")
        );

        let gain = recorder
            .entities()
            .iter()
            .find(|entity| entity.id.as_str() == "param_gain")
            .unwrap();
        assert_eq!(gain.kind, EntityKind::Parameter);
        assert!(matches!(
            gain.parameter_default.as_ref(),
            Some(ParameterDefault::Double(value)) if value.as_str() == "1.5"
        ));

        assert_eq!(recorder.callback_effects().len(), 3);
        assert!(recorder.callback_effects().iter().any(|effect| {
            effect.callback_id.as_str() == "cb_cmd"
                && effect.kind == CallbackEffectKind::Publishes
                && effect.entity_id.as_str() == "pub_status"
        }));
        assert!(recorder.callback_effects().iter().any(|effect| {
            effect.callback_id.as_str() == "cb_nav_accepted"
                && effect.kind == CallbackEffectKind::Writes
                && effect.entity_id.as_str() == "param_gain"
        }));
    }

    #[cfg(feature = "std")]
    #[test]
    fn component_api_json_contains_planner_callback_links() {
        let mut recorder = MetadataRecorder::<4, 12, 4>::new();
        record_node_metadata::<RobotComponent>(&mut recorder).unwrap();

        let json = recorder
            .to_source_metadata_json(&crate::SourceMetadataExport::new(
                "demo_robot",
                RobotComponent::NAME,
            ))
            .unwrap();

        assert!(json.contains("\"callbacks\":["));
        assert!(json.contains("\"id\":\"cb_cmd\",\"kind\":\"subscription\""));
        assert!(json.contains("\"id\":\"cb_reset\",\"kind\":\"service\""));
        assert!(json.contains("\"id\":\"cb_nav_goal\",\"kind\":\"action_goal\""));
        assert!(json.contains("\"id\":\"cb_nav_cancel\",\"kind\":\"action_cancel\""));
        assert!(json.contains("\"id\":\"cb_nav_accepted\",\"kind\":\"action_accepted\""));
        assert!(json.contains("\"kind\":\"publishes\",\"entity\":\"pub_status\""));
        assert!(json.contains("\"kind\":\"reads_parameter\",\"entity\":\"param_gain\""));
        assert!(json.contains("\"kind\":\"writes_parameter\",\"entity\":\"param_gain\""));
        assert!(json.contains("\"goal_callback\":\"cb_nav_goal\""));
        assert!(json.contains("\"cancel_callback\":\"cb_nav_cancel\""));
        assert!(json.contains("\"accepted_callback\":\"cb_nav_accepted\""));
    }

    // W.5.1 — an executable component callback runs its body: mutates state +
    // publishes immediately through the resolver (the substrate the generator
    // will wire). `TalkerComponent` already impls `Node` (declarative);
    // here it also impls `ExecutableNode`.
    impl ExecutableNode for TalkerComponent {
        type State = u32;

        fn init() -> u32 {
            0
        }

        fn on_callback(state: &mut u32, callback: CallbackId<'_>, ctx: &mut CallbackCtx<'_>) {
            if callback.as_str() == "on_tick" {
                *state += 1;
                // Publish through the declared publisher entity.
                let _ = ctx.publish::<TestMsg, 64>(EntityId::new("pub_chatter"), &TestMsg);
            }
        }
    }

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
        TalkerComponent::on_callback(&mut state, CallbackId::new("other"), &mut ctx);
        assert_eq!(state, 0);
        assert!(resolver.last.borrow().is_none());

        // The bound callback bumps state + publishes through "pub_chatter".
        TalkerComponent::on_callback(&mut state, CallbackId::new("on_tick"), &mut ctx);
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
            assert!(ctx.set_cancel_response(CancelResponse::Ok).is_err());
        }
        assert!(matches!(gr, GoalResponse::AcceptAndExecute));

        let mut cr = CancelResponse::Rejected;
        {
            let mut ctx = CallbackCtx::with_cancel_decision(&[], &resolver, &mut cr);
            ctx.set_cancel_response(CancelResponse::Ok).unwrap();
        }
        assert!(matches!(cr, CancelResponse::Ok));

        // A timer/sub ctx (no decision sink) rejects both.
        let mut ctx3 = CallbackCtx::new(&[], &resolver);
        assert!(ctx3.set_goal_response(GoalResponse::Reject).is_err());
        assert!(ctx3.set_cancel_response(CancelResponse::Ok).is_err());
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
    #[test]
    fn node_dispatch_default_is_inline() {
        struct Dummy;
        impl Node for Dummy {
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
    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
    mod dispatch_probe_macro_test {
        // `extern crate self as nros;` at the crate root (in `lib.rs`,
        // `cfg(test)`-gated) lets the `::nros::*` paths the macro emits
        // resolve in-crate.
        use super::*;

        pub struct DispatchProbe;

        impl Node for DispatchProbe {
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
                _callback: CallbackId<'_>,
                _ctx: &mut CallbackCtx<'_>,
            ) {
            }
        }

        // Emits both the per-pkg `register` wrapper AND the new
        // `__nros_node_nros_dispatch_strategy` ABI symbol.
        nros_macros::node!(DispatchProbe);
    }

    #[cfg(all(feature = "alloc", feature = "rmw-cffi"))]
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
        // `<Type as Node>::DISPATCH as u8`, not a hard-coded zero.
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

    #[test]
    fn create_subscription_static_returns_tag_matching_topic() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let mut node = context
            .create_node(NodeId::new("node"), NodeOptions::new("listener"))
            .unwrap();
        let tag = node
            .create_subscription_static::<TestMsg>("/chatter")
            .unwrap();

        assert_eq!(tag.as_str(), "/chatter");
        assert!(tag == CallbackId::new("/chatter"));
        assert_eq!(recorder.entities().len(), 1);
        let entity = &recorder.entities()[0];
        assert_eq!(entity.kind, EntityKind::Subscription);
        assert_eq!(entity.source_name.as_str(), "/chatter");
        assert_eq!(
            entity.callback_id.as_ref().map(|id| id.as_str()),
            Some("/chatter")
        );
    }

    #[test]
    fn create_service_static_returns_tag() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let mut node = context
            .create_node(NodeId::new("node"), NodeOptions::new("server"))
            .unwrap();
        let tag = node
            .create_service_static::<TestService>("/add_two_ints")
            .unwrap();

        assert_eq!(tag.as_str(), "/add_two_ints");
        assert!(tag == CallbackId::new("/add_two_ints"));
        assert_eq!(recorder.entities().len(), 1);
        let entity = &recorder.entities()[0];
        assert_eq!(entity.kind, EntityKind::ServiceServer);
        assert_eq!(entity.source_name.as_str(), "/add_two_ints");
        assert_eq!(
            entity.callback_id.as_ref().map(|id| id.as_str()),
            Some("/add_two_ints")
        );
    }

    #[test]
    fn create_action_static_returns_tag() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        let mut context = NodeContext::new("test", &mut recorder);
        let mut node = context
            .create_node(NodeId::new("node"), NodeOptions::new("server"))
            .unwrap();
        let tag = node
            .create_action_static::<TestAction>("/fibonacci")
            .unwrap();

        assert_eq!(tag.as_str(), "/fibonacci");
        assert!(tag == CallbackId::new("/fibonacci"));
        assert_eq!(recorder.entities().len(), 1);
        let entity = &recorder.entities()[0];
        assert_eq!(entity.kind, EntityKind::ActionServer);
        assert_eq!(entity.source_name.as_str(), "/fibonacci");
        assert_eq!(
            entity.callback_id.as_ref().map(|id| id.as_str()),
            Some("/fibonacci")
        );
        assert_eq!(
            entity
                .action_cancel_callback_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("/fibonacci")
        );
        assert_eq!(
            entity
                .action_accepted_callback_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("/fibonacci")
        );
    }
}
