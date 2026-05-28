//! Rust component API shared by metadata discovery and generated runtimes.

use core::marker::PhantomData;

use crate::{
    CallbackId, EntityId, ParameterType, QosSettings, RosAction, RosMessage, RosService,
    TimerDuration,
    component_metadata::{
        CallbackEffectKind, CallbackEffectMetadata, ComponentMetadataError, EntityKind,
        EntityMetadata, EntityMetadataSpec, MetadataRecorder, MetadataString, NodeId,
        ParameterDefault, SourceLocationMetadata, copy_str, entity_metadata,
    },
    heapless::Vec,
};

/// Stable symbol exported by [`nros::component!`](crate::component!).
pub const COMPONENT_EXPORT_SYMBOL: &str = "__nros_component_register";

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

/// Run component registration against any component runtime.
pub fn register_component<C: Component>(runtime: &mut dyn ComponentRuntime) -> ComponentResult<()> {
    let mut context = ComponentContext::new(C::NAME, runtime);
    C::register(&mut context)
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
}
