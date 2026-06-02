//! Rust component API shared by metadata discovery and generated runtimes.

use core::marker::PhantomData;

use crate::{
    CallbackId, CancelResponse, EntityId, GoalId, GoalResponse, GoalStatus, ParameterType,
    QosSettings, RosAction, RosMessage, RosService, TimerDuration,
    component_metadata::{
        CallbackEffectKind, CallbackEffectMetadata, ComponentMetadataError, EntityKind,
        EntityMetadata, EntityMetadataSpec, MetadataRecorder, MetadataString, NodeId,
        ParameterDefault, SourceLocationMetadata, copy_str, entity_metadata,
    },
    heapless::Vec,
};

// Phase 212.N.7 step-6 closing sweep — `component_register_symbol`
// removed. It built the legacy `__nros_component_<pkg>_register`
// symbol name for the M.5.a BSP baker to look up by literal. step-6
// retired the macro emit + step-4 deleted the FreeRTOS BSP baker
// crate that was the sole live consumer. The Phase 212.N Entry pkg
// path calls `<pkg>::register(runtime)` through the path API, so this
// helper has no live callers.

/// Clear diagnostic for packages missing [`nros::component!`](crate::component!).
pub const MISSING_COMPONENT_EXPORT_ERROR: &str = "package has no exported nros component";

/// Result type for component declarations.
pub type ComponentResult<T = ()> = Result<T, ComponentError>;

/// Component declaration error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentError {
    /// Metadata recorder rejected the declaration.
    Metadata(ComponentMetadataError),
    /// Host/runtime discovery could not find `nros::component!` export.
    MissingExport,
    /// Generated runtime rejected the declaration.
    Runtime,
}

impl ComponentError {
    /// Human-readable static message for diagnostics that cross FFI/plugin boundaries.
    pub const fn message(self) -> &'static str {
        match self {
            Self::Metadata(ComponentMetadataError::Capacity) => {
                "component metadata capacity exceeded"
            }
            Self::Metadata(ComponentMetadataError::NameTooLong) => {
                "component metadata name too long"
            }
            Self::Metadata(ComponentMetadataError::UnknownNode) => {
                "component entity references an unknown node"
            }
            Self::Metadata(ComponentMetadataError::UnknownEntity) => {
                "component callback effect references an unknown entity"
            }
            Self::Metadata(ComponentMetadataError::DuplicateId) => {
                "component metadata contains a duplicate stable ID"
            }
            Self::MissingExport => MISSING_COMPONENT_EXPORT_ERROR,
            Self::Runtime => "component runtime rejected declaration",
        }
    }
}

impl From<ComponentMetadataError> for ComponentError {
    fn from(value: ComponentMetadataError) -> Self {
        Self::Metadata(value)
    }
}

/// Rust component entry point.
pub trait Component {
    /// Source component name used in metadata and diagnostics.
    const NAME: &'static str;

    /// Declare nodes, entities, callbacks, params, and optional effects.
    fn register(context: &mut ComponentContext<'_>) -> ComponentResult<()>;
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
pub trait ComponentRuntime {
    /// Declare a component node.
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> ComponentResult<()>;

    /// Declare a publisher, subscription, timer, service, action, or parameter.
    fn create_entity(&mut self, metadata: EntityMetadata) -> ComponentResult<()>;

    /// Add optional callback effect metadata.
    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> ComponentResult<()>;
}

impl<const MAX_NODES: usize, const MAX_ENTITIES: usize, const MAX_CALLBACKS: usize> ComponentRuntime
    for MetadataRecorder<MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> ComponentResult<()> {
        self.push_node(id, options.name, options.namespace, options.domain_id)?;
        Ok(())
    }

    fn create_entity(&mut self, metadata: EntityMetadata) -> ComponentResult<()> {
        self.push_entity(metadata)?;
        Ok(())
    }

    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> ComponentResult<()> {
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
pub trait ComponentNodeRuntime {
    /// Concrete node handle owned by the runtime executor.
    type NodeHandle: Copy + Eq;

    /// Create a runtime node from source-level component options.
    fn build_component_node(
        &mut self,
        id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> ComponentResult<Self::NodeHandle>;
}

/// Recorded runtime node mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentRuntimeNode<H: Copy + Eq> {
    stable_id: MetadataString,
    handle: H,
}

impl<H: Copy + Eq> ComponentRuntimeNode<H> {
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
pub struct ComponentRuntimeAdapter<
    'a,
    R: ComponentNodeRuntime + ?Sized,
    const MAX_NODES: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_NODES },
    const MAX_ENTITIES: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_ENTITIES },
    const MAX_CALLBACKS: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_CALLBACKS },
> {
    node_runtime: &'a mut R,
    nodes: Vec<ComponentRuntimeNode<R::NodeHandle>, MAX_NODES>,
    entities: Vec<EntityMetadata, MAX_ENTITIES>,
    callback_effects: Vec<CallbackEffectMetadata, MAX_CALLBACKS>,
}

impl<
    'a,
    R: ComponentNodeRuntime + ?Sized,
    const MAX_NODES: usize,
    const MAX_ENTITIES: usize,
    const MAX_CALLBACKS: usize,
> ComponentRuntimeAdapter<'a, R, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
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
    pub fn nodes(&self) -> &[ComponentRuntimeNode<R::NodeHandle>] {
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
            .map(ComponentRuntimeNode::handle)
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
    R: ComponentNodeRuntime + ?Sized,
    const MAX_NODES: usize,
    const MAX_ENTITIES: usize,
    const MAX_CALLBACKS: usize,
> ComponentRuntime for ComponentRuntimeAdapter<'_, R, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    fn create_node(&mut self, id: NodeId<'_>, options: NodeOptions<'_>) -> ComponentResult<()> {
        if self.contains_node(id.as_str()) {
            return Err(ComponentMetadataError::DuplicateId.into());
        }
        let handle = self.node_runtime.build_component_node(id, options)?;
        self.nodes
            .push(ComponentRuntimeNode {
                stable_id: copy_str(id.as_str())?,
                handle,
            })
            .map_err(|_| ComponentError::Metadata(ComponentMetadataError::Capacity))?;
        Ok(())
    }

    fn create_entity(&mut self, metadata: EntityMetadata) -> ComponentResult<()> {
        if !self.contains_node(metadata.node_id.as_str()) {
            return Err(ComponentMetadataError::UnknownNode.into());
        }
        if self.contains_entity(metadata.id.as_str()) {
            return Err(ComponentMetadataError::DuplicateId.into());
        }
        self.entities
            .push(metadata)
            .map_err(|_| ComponentError::Metadata(ComponentMetadataError::Capacity))?;
        Ok(())
    }

    fn record_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> ComponentResult<()> {
        if !self.contains_entity(entity_id.as_str()) {
            return Err(ComponentMetadataError::UnknownEntity.into());
        }
        self.callback_effects
            .push(CallbackEffectMetadata {
                callback_id: copy_str(callback_id.as_str())?,
                kind,
                entity_id: copy_str(entity_id.as_str())?,
            })
            .map_err(|_| ComponentError::Metadata(ComponentMetadataError::Capacity))?;
        Ok(())
    }
}

#[cfg(feature = "rmw-cffi")]
impl ComponentNodeRuntime for crate::Executor {
    type NodeHandle = nros_node::executor::NodeId;

    fn build_component_node(
        &mut self,
        _id: NodeId<'_>,
        options: NodeOptions<'_>,
    ) -> ComponentResult<Self::NodeHandle> {
        self.node_builder(options.name)
            .namespace(options.namespace)
            .domain_id(options.domain_id)
            .build()
            .map_err(|_| ComponentError::Runtime)
    }
}

/// Runtime adapter backed by [`Executor`](crate::Executor).
#[cfg(feature = "rmw-cffi")]
pub type ComponentExecutorRuntime<
    'a,
    const MAX_NODES: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_NODES },
    const MAX_ENTITIES: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_ENTITIES },
    const MAX_CALLBACKS: usize = { crate::component_metadata::DEFAULT_MAX_METADATA_CALLBACKS },
> = ComponentRuntimeAdapter<'a, crate::Executor, MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>;

/// Component declaration context. Does not own middleware transport.
pub struct ComponentContext<'a, R: ComponentRuntime + ?Sized = dyn ComponentRuntime + 'a> {
    component_name: &'static str,
    runtime: &'a mut R,
}

impl<'a, R: ComponentRuntime + ?Sized> ComponentContext<'a, R> {
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

    /// Declare a node. Stable node ID is required in component mode.
    pub fn create_node<'id>(
        &mut self,
        id: NodeId<'id>,
        options: NodeOptions<'_>,
    ) -> ComponentResult<ComponentNode<'_, 'id, R>> {
        self.runtime.create_node(id, options)?;
        Ok(ComponentNode {
            runtime: self.runtime,
            id,
        })
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
pub struct ComponentNode<'ctx, 'id, R: ComponentRuntime + ?Sized = dyn ComponentRuntime + 'ctx> {
    runtime: &'ctx mut R,
    id: NodeId<'id>,
}

impl<'ctx, 'id, R: ComponentRuntime + ?Sized> ComponentNode<'ctx, 'id, R> {
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
    ) -> ComponentResult<ComponentPublisher<'entity, M>> {
        self.create_publisher_with_qos::<M>(id, topic, QosSettings::default())
    }

    /// Declare a publisher with explicit QoS.
    #[track_caller]
    pub fn create_publisher_with_qos<'entity, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
        qos: QosSettings,
    ) -> ComponentResult<ComponentPublisher<'entity, M>> {
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
        Ok(ComponentPublisher::new(id))
    }

    /// Declare a subscription. Stable subscription and callback IDs are required.
    #[track_caller]
    pub fn create_subscription<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> ComponentResult<ComponentSubscription<'entity, M>> {
        self.create_subscription_with_qos::<M>(id, callback_id, topic, QosSettings::default())
    }

    /// Declare a subscription with explicit QoS.
    #[track_caller]
    pub fn create_subscription_with_qos<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QosSettings,
    ) -> ComponentResult<ComponentSubscription<'entity, M>> {
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
        Ok(ComponentSubscription::new(id))
    }

    /// Declare a timer. Stable timer and callback IDs are required.
    #[track_caller]
    pub fn create_timer<'entity, 'callback>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> ComponentResult<ComponentTimer<'entity>> {
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
        Ok(ComponentTimer::new(id))
    }

    /// Declare a service server. Stable service and callback IDs are required.
    #[track_caller]
    pub fn create_service_server<'entity, 'callback, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        service_name: &str,
    ) -> ComponentResult<ComponentServiceServer<'entity, S>> {
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
        Ok(ComponentServiceServer::new(id))
    }

    /// Declare a service client. Stable service client ID is required.
    #[track_caller]
    pub fn create_service_client<'entity, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        service_name: &str,
    ) -> ComponentResult<ComponentServiceClient<'entity, S>> {
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
        Ok(ComponentServiceClient::new(id))
    }

    /// Declare an action server. Stable action and callback IDs are required.
    #[track_caller]
    pub fn create_action_server<'entity, 'callback, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        action_name: &str,
    ) -> ComponentResult<ComponentActionServer<'entity, A>> {
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
    ) -> ComponentResult<ComponentActionServer<'entity, A>> {
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
        Ok(ComponentActionServer::new(id))
    }

    /// Declare an action client. Stable action client ID is required.
    #[track_caller]
    pub fn create_action_client<'entity, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        action_name: &str,
    ) -> ComponentResult<ComponentActionClient<'entity, A>> {
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
        Ok(ComponentActionClient::new(id))
    }

    /// Declare a parameter. Stable parameter ID is required.
    #[track_caller]
    pub fn declare_parameter<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        parameter_type: ParameterType,
    ) -> ComponentResult<ComponentParameter<'entity>> {
        self.declare_parameter_with_default(id, name, ParameterDefault::for_type(parameter_type)?)
    }

    /// Declare a parameter with a concrete source default.
    #[track_caller]
    pub fn declare_parameter_with_default<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        default: ParameterDefault,
    ) -> ComponentResult<ComponentParameter<'entity>> {
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
        Ok(ComponentParameter::new(id))
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
pub struct CallbackEffects<'ctx, 'id, R: ComponentRuntime + ?Sized = dyn ComponentRuntime + 'ctx> {
    runtime: &'ctx mut R,
    id: CallbackId<'id>,
}

impl<'ctx, 'id, R: ComponentRuntime + ?Sized> CallbackEffects<'ctx, 'id, R> {
    /// Record that callback reads from an entity.
    pub fn reads(self, entity_id: EntityId<'_>) -> ComponentResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Reads, entity_id)?;
        Ok(self)
    }

    /// Record that callback publishes via an entity.
    pub fn publishes(self, entity_id: EntityId<'_>) -> ComponentResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Publishes, entity_id)?;
        Ok(self)
    }

    /// Record that callback writes to an entity or parameter.
    pub fn writes(self, entity_id: EntityId<'_>) -> ComponentResult<Self> {
        self.runtime
            .record_callback_effect(self.id, CallbackEffectKind::Writes, entity_id)?;
        Ok(self)
    }
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
    };
}

component_handle!(ComponentPublisher, M);
component_handle!(ComponentSubscription, M);
component_handle!(ComponentServiceServer, S);
component_handle!(ComponentServiceClient, S);
component_handle!(ComponentActionServer, A);
component_handle!(ComponentActionClient, A);
component_handle!(ComponentTimer);
component_handle!(ComponentParameter);

// ============================================================================
// Phase 172 W.5.1 — executable component layer (callback bodies)
// ============================================================================
//
// The declarative `Component::register` above stays the planning/metadata SSOT.
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
    /// `Err(ComponentError::Runtime)` if no such publisher is registered or the
    /// transport rejects the write.
    fn publish_raw(&self, entity_id: &str, data: &[u8]) -> ComponentResult<()>;
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
/// Service / action-result callbacks additionally carry a [`ReplySink`] the body
/// fills via [`reply`](Self::reply); action goal / cancel callbacks carry a
/// [`DecisionSink`] the body fills via
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
    pub fn set_goal_response(&mut self, response: GoalResponse) -> ComponentResult<()> {
        match &mut self.decision {
            Some(DecisionSink::Goal(slot)) => {
                **slot = response;
                Ok(())
            }
            _ => Err(ComponentError::Runtime),
        }
    }

    /// Set the action cancel-callback's accept/reject decision (W.5.3). `Err` when
    /// the callback is not a cancel decision.
    pub fn set_cancel_response(&mut self, response: CancelResponse) -> ComponentResult<()> {
        match &mut self.decision {
            Some(DecisionSink::Cancel(slot)) => {
                **slot = response;
                Ok(())
            }
            _ => Err(ComponentError::Runtime),
        }
    }

    /// Write the service / action reply as raw CDR bytes (W.5.3). `Err` when the
    /// callback has no reply sink (timer / subscription) or the reply exceeds the
    /// lent buffer.
    pub fn reply_raw(&mut self, data: &[u8]) -> ComponentResult<()> {
        let sink = self.reply.as_mut().ok_or(ComponentError::Runtime)?;
        if data.len() > sink.buf.len() {
            return Err(ComponentError::Runtime);
        }
        sink.buf[..data.len()].copy_from_slice(data);
        *sink.written = data.len();
        Ok(())
    }

    /// Serialize `msg` and write it as the service / action reply (W.5.3).
    pub fn reply<M: RosMessage, const N: usize>(&mut self, msg: &M) -> ComponentResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| ComponentError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
        let len = writer.position();
        self.reply_raw(&buf[..len])
    }

    /// Raw CDR payload of the triggering message / request. Empty for timers.
    pub fn payload(&self) -> &[u8] {
        self.payload
    }

    /// Deserialize the triggering payload as `M` (subscription / service-request
    /// bodies). `Err` if the payload is malformed for `M`.
    pub fn message<M: RosMessage>(&self) -> ComponentResult<M> {
        let mut reader =
            crate::CdrReader::new_with_header(self.payload).map_err(|_| ComponentError::Runtime)?;
        M::deserialize(&mut reader).map_err(|_| ComponentError::Runtime)
    }

    /// Publish raw CDR bytes through the named publisher entity (immediate).
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> ComponentResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
    /// `N` must be ≥ the CDR-encoded size of `msg`; the generated runtime picks
    /// it from the message type.
    pub fn publish<M: RosMessage, const N: usize>(
        &self,
        publisher: EntityId<'_>,
        msg: &M,
    ) -> ComponentResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| ComponentError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
        let len = writer.position();
        self.publish_raw(publisher, &buf[..len])
    }
}

/// The executable counterpart of [`Component`] (W.5.1).
///
/// `register` (declarative) stays the planning SSOT; this binds runnable
/// bodies. The generated runtime builds [`State`](Self::State) once via
/// [`init`](Self::init), then routes every fired callback to
/// [`on_callback`](Self::on_callback). Trait-dispatch (no boxed `dyn`, no
/// `alloc`) keeps it `no_std`.
/// Executor-backed action operations a [`TickCtx`] drives (W.5.6).
///
/// Action result/feedback need `&mut Executor` (`complete_goal_raw` /
/// `publish_feedback_raw`), which a mid-spin *callback* can't hold (the executor
/// is borrowed) — so they run from [`ExecutableComponent::tick`], between spins.
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
    ) -> ComponentResult<()>;

    /// Publish raw CDR feedback for `goal_id` on action `action_entity`.
    fn publish_feedback_raw(
        &mut self,
        action_entity: &str,
        goal_id: &GoalId,
        feedback: &[u8],
    ) -> ComponentResult<()>;

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
/// callback can't hold. They run from [`ExecutableComponent::tick`], between
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
    ) -> ComponentResult<usize>;

    /// Send an action-client goal request on `action_entity` carrying
    /// CDR `goal_cdr`; return the assigned [`GoalId`] (server-stamped on
    /// the goal-accept response). Result + feedback streams arrive via
    /// callback dispatch — not this method.
    fn send_goal_raw(&mut self, action_entity: &str, goal_cdr: &[u8]) -> ComponentResult<GoalId>;
}

/// Context handed to [`ExecutableComponent::tick`] (W.5.6 + M-F.4): the per-spin
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
    pub fn publish_raw(&self, publisher: EntityId<'_>, data: &[u8]) -> ComponentResult<()> {
        self.publishers.publish_raw(publisher.as_str(), data)
    }

    /// Serialize `msg` into an `N`-byte stack buffer and publish it (immediate).
    pub fn publish<M: RosMessage, const N: usize>(
        &self,
        publisher: EntityId<'_>,
        msg: &M,
    ) -> ComponentResult<()> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| ComponentError::Runtime)?;
        msg.serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
        let len = writer.position();
        self.publish_raw(publisher, &buf[..len])
    }

    /// Complete an action goal with a typed result (W.5.6 — needs the executor,
    /// hence tick-only).
    pub fn complete_goal<R: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal_id: &GoalId,
        status: GoalStatus,
        result: &R,
    ) -> ComponentResult<()> {
        // Header-less inner CDR: the executor's `complete_goal_raw` frames the
        // outer envelope (matches the typed `ActionServerHandle::complete_goal`).
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        result
            .serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
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
    ) -> ComponentResult<()> {
        // Header-less inner CDR: the executor's `publish_feedback_raw` frames the
        // outer envelope (matches the typed `ActionServerHandle::publish_feedback`).
        let mut buf = [0u8; N];
        let mut writer = crate::CdrWriter::new(&mut buf);
        feedback
            .serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
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
    ) -> ComponentResult<usize> {
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
    ) -> ComponentResult<Resp> {
        let mut req_buf = [0u8; REQ_N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut req_buf).map_err(|_| ComponentError::Runtime)?;
        request
            .serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
        let req_len = writer.position();

        let mut resp_buf = [0u8; RESP_N];
        let resp_len =
            self.clients
                .call_raw(service.as_str(), &req_buf[..req_len], &mut resp_buf)?;

        let mut reader = crate::CdrReader::new_with_header(&resp_buf[..resp_len])
            .map_err(|_| ComponentError::Runtime)?;
        Resp::deserialize(&mut reader).map_err(|_| ComponentError::Runtime)
    }

    /// Send a raw-CDR action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). Result + feedback streams arrive
    /// via callback dispatch; this method only kicks off the request.
    pub fn send_goal_raw(
        &mut self,
        action: EntityId<'_>,
        goal_cdr: &[u8],
    ) -> ComponentResult<GoalId> {
        self.clients.send_goal_raw(action.as_str(), goal_cdr)
    }

    /// Send a typed action-client goal and return the assigned
    /// [`GoalId`] (M-F.4 — tick-only). `N` stack-sizes the goal CDR
    /// buffer.
    pub fn send_goal<G: RosMessage, const N: usize>(
        &mut self,
        action: EntityId<'_>,
        goal: &G,
    ) -> ComponentResult<GoalId> {
        let mut buf = [0u8; N];
        let mut writer =
            crate::CdrWriter::new_with_header(&mut buf).map_err(|_| ComponentError::Runtime)?;
        goal.serialize(&mut writer)
            .map_err(|_| ComponentError::Runtime)?;
        let len = writer.position();
        self.clients.send_goal_raw(action.as_str(), &buf[..len])
    }
}

pub trait ExecutableComponent: Component {
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

/// Emit a no-op [`ExecutableComponent`] impl for a declarative-only component
/// (W.5.1). The generated runtime calls `on_callback` unconditionally, so a
/// component instantiated into a generated binary must impl `ExecutableComponent`;
/// components without callback bodies use this to satisfy that contract:
///
/// ```ignore
/// pub struct Component;
/// impl nros::Component for Component { /* register(...) */ }
/// nros::declarative_component!(Component);
/// ```
#[macro_export]
macro_rules! declarative_component {
    ($ty:ty) => {
        impl $crate::ExecutableComponent for $ty {
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
pub fn register_component<C: Component>(runtime: &mut dyn ComponentRuntime) -> ComponentResult<()> {
    let mut context = ComponentContext::new(C::NAME, runtime);
    C::register(&mut context)
}

/// Phase 212.M.5.a.4 internal — `Box`-erase a freshly built component
/// `State` to the type-erased `*mut ()` ABI the BSP path uses. Called
/// only from the `nros::component!()` macro emit; not public API.
///
/// The returned pointer is a leaked `Box`; the BSP runtime keeps it
/// alive for the firmware lifetime (embedded slots never deallocate).
#[cfg(feature = "alloc")]
#[doc(hidden)]
pub fn __private_component_state_into_raw<C: ExecutableComponent>(state: C::State) -> *mut () {
    extern crate alloc;
    alloc::boxed::Box::into_raw(alloc::boxed::Box::new(state)) as *mut ()
}

/// Run component registration against an in-memory metadata recorder.
pub fn record_component_metadata<C: Component>(
    recorder: &mut dyn ComponentRuntime,
) -> ComponentResult<()> {
    register_component::<C>(recorder)
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

    impl ComponentNodeRuntime for FakeNodeRuntime {
        type NodeHandle = u8;

        fn build_component_node(
            &mut self,
            _id: NodeId<'_>,
            options: NodeOptions<'_>,
        ) -> ComponentResult<Self::NodeHandle> {
            self.created
                .push(copy_str(options.name)?)
                .map_err(|_| ComponentError::Metadata(ComponentMetadataError::Capacity))?;
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

        const ACTION_NAME: &'static str = "test_msgs::action::dds_::Test_";
        const ACTION_HASH: &'static str = "test_action_hash";
    }

    struct TalkerComponent;

    impl Component for TalkerComponent {
        const NAME: &'static str = "talker_component";

        fn register(context: &mut ComponentContext<'_>) -> ComponentResult<()> {
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
        record_component_metadata::<TalkerComponent>(&mut recorder).unwrap();

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
        let mut runtime = ComponentRuntimeAdapter::<_, 2, 8, 4>::new(&mut node_runtime);

        register_component::<TalkerComponent>(&mut runtime).unwrap();

        assert_eq!(runtime.nodes().len(), 1);
        assert_eq!(runtime.nodes()[0].stable_id(), "node");
        assert_eq!(runtime.node_handle(NodeId::new("node")), Some(0));
        assert_eq!(runtime.entities().len(), 4);
        assert_eq!(runtime.callback_effects().len(), 2);
    }

    #[test]
    fn runtime_adapter_rejects_duplicate_nodes_and_unknown_effect_entities() {
        let mut node_runtime = FakeNodeRuntime::default();
        let mut runtime = ComponentRuntimeAdapter::<_, 1, 1, 1>::new(&mut node_runtime);
        runtime
            .create_node(NodeId::new("node"), NodeOptions::new("talker"))
            .unwrap();

        assert_eq!(
            runtime.create_node(NodeId::new("node"), NodeOptions::new("other")),
            Err(ComponentError::Metadata(
                ComponentMetadataError::DuplicateId
            ))
        );
        assert_eq!(
            runtime.record_callback_effect(
                CallbackId::new("cb"),
                CallbackEffectKind::Reads,
                EntityId::new("missing")
            ),
            Err(ComponentError::Metadata(
                ComponentMetadataError::UnknownEntity
            ))
        );
    }

    #[test]
    fn component_rejects_effect_for_unknown_entity() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        let mut context = ComponentContext::new("test", &mut recorder);
        let result = context
            .callback(CallbackId::new("cb"))
            .reads(EntityId::new("missing"));
        assert!(matches!(
            result,
            Err(ComponentError::Metadata(
                ComponentMetadataError::UnknownEntity
            ))
        ));
    }

    #[test]
    fn component_missing_export_error_message_is_clear() {
        assert_eq!(
            ComponentError::MissingExport.message(),
            MISSING_COMPONENT_EXPORT_ERROR
        );
        assert_eq!(
            ComponentError::MissingExport.message(),
            "package has no exported nros component"
        );
    }

    struct RobotComponent;

    impl Component for RobotComponent {
        const NAME: &'static str = "robot_component";

        fn register(context: &mut ComponentContext<'_>) -> ComponentResult<()> {
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

    #[test]
    fn component_api_records_multi_node_services_actions_and_defaults() {
        let mut recorder = MetadataRecorder::<4, 12, 4>::new();
        record_component_metadata::<RobotComponent>(&mut recorder).unwrap();

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
        record_component_metadata::<RobotComponent>(&mut recorder).unwrap();

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
    // will wire). `TalkerComponent` already impls `Component` (declarative);
    // here it also impls `ExecutableComponent`.
    impl ExecutableComponent for TalkerComponent {
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
            fn publish_raw(&self, entity_id: &str, data: &[u8]) -> ComponentResult<()> {
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
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> ComponentResult<()> {
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
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> ComponentResult<()> {
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
            fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> ComponentResult<()> {
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
            ) -> ComponentResult<()> {
                self.completed = true;
                Ok(())
            }
            fn publish_feedback_raw(
                &mut self,
                _action_entity: &str,
                _goal_id: &GoalId,
                _feedback: &[u8],
            ) -> ComponentResult<()> {
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
            ) -> ComponentResult<usize> {
                Err(ComponentError::Runtime)
            }
            fn send_goal_raw(&mut self, _action: &str, _goal: &[u8]) -> ComponentResult<GoalId> {
                Err(ComponentError::Runtime)
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
}
