//! Rust component API shared by metadata discovery and generated runtimes.

use core::marker::PhantomData;

use crate::{
    CallbackId, EntityId, ParameterType, QosSettings, RosAction, RosMessage, RosService,
    TimerDuration,
    component_metadata::{
        CallbackEffectKind, ComponentMetadataError, EntityKind, EntityMetadata, MetadataRecorder,
        NodeId, copy_str, entity_metadata,
    },
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
    pub fn create_publisher<'entity, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
    ) -> ComponentResult<ComponentPublisher<'entity, M>> {
        self.create_publisher_with_qos::<M>(id, topic, QosSettings::default())
    }

    /// Declare a publisher with explicit QoS.
    pub fn create_publisher_with_qos<'entity, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        topic: &str,
        qos: QosSettings,
    ) -> ComponentResult<ComponentPublisher<'entity, M>> {
        let metadata = entity_metadata(
            id,
            self.id,
            EntityKind::Publisher,
            topic,
            M::TYPE_NAME,
            M::TYPE_HASH,
            qos,
        )?;
        self.runtime.create_entity(metadata)?;
        Ok(ComponentPublisher::new(id))
    }

    /// Declare a subscription. Stable subscription and callback IDs are required.
    pub fn create_subscription<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
    ) -> ComponentResult<ComponentSubscription<'entity, M>> {
        self.create_subscription_with_qos::<M>(id, callback_id, topic, QosSettings::default())
    }

    /// Declare a subscription with explicit QoS.
    pub fn create_subscription_with_qos<'entity, 'callback, M: RosMessage>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        topic: &str,
        qos: QosSettings,
    ) -> ComponentResult<ComponentSubscription<'entity, M>> {
        let mut metadata = entity_metadata(
            id,
            self.id,
            EntityKind::Subscription,
            topic,
            M::TYPE_NAME,
            M::TYPE_HASH,
            qos,
        )?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        self.runtime.create_entity(metadata)?;
        Ok(ComponentSubscription::new(id))
    }

    /// Declare a timer. Stable timer and callback IDs are required.
    pub fn create_timer<'entity, 'callback>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        period: TimerDuration,
    ) -> ComponentResult<ComponentTimer<'entity>> {
        let mut metadata = entity_metadata(
            id,
            self.id,
            EntityKind::Timer,
            "",
            "",
            "",
            QosSettings::default(),
        )?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        metadata.period_ms = Some(period.as_millis());
        self.runtime.create_entity(metadata)?;
        Ok(ComponentTimer::new(id))
    }

    /// Declare a service server. Stable service and callback IDs are required.
    pub fn create_service_server<'entity, 'callback, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        service_name: &str,
    ) -> ComponentResult<ComponentServiceServer<'entity, S>> {
        let mut metadata = entity_metadata(
            id,
            self.id,
            EntityKind::ServiceServer,
            service_name,
            S::SERVICE_NAME,
            S::SERVICE_HASH,
            QosSettings::default(),
        )?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        self.runtime.create_entity(metadata)?;
        Ok(ComponentServiceServer::new(id))
    }

    /// Declare a service client. Stable service client ID is required.
    pub fn create_service_client<'entity, S: RosService>(
        &mut self,
        id: EntityId<'entity>,
        service_name: &str,
    ) -> ComponentResult<ComponentServiceClient<'entity, S>> {
        let metadata = entity_metadata(
            id,
            self.id,
            EntityKind::ServiceClient,
            service_name,
            S::SERVICE_NAME,
            S::SERVICE_HASH,
            QosSettings::default(),
        )?;
        self.runtime.create_entity(metadata)?;
        Ok(ComponentServiceClient::new(id))
    }

    /// Declare an action server. Stable action and callback IDs are required.
    pub fn create_action_server<'entity, 'callback, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        callback_id: CallbackId<'callback>,
        action_name: &str,
    ) -> ComponentResult<ComponentActionServer<'entity, A>> {
        let mut metadata = entity_metadata(
            id,
            self.id,
            EntityKind::ActionServer,
            action_name,
            A::ACTION_NAME,
            A::ACTION_HASH,
            QosSettings::default(),
        )?;
        metadata.callback_id = Some(copy_str(callback_id.as_str())?);
        self.runtime.create_entity(metadata)?;
        Ok(ComponentActionServer::new(id))
    }

    /// Declare an action client. Stable action client ID is required.
    pub fn create_action_client<'entity, A: RosAction>(
        &mut self,
        id: EntityId<'entity>,
        action_name: &str,
    ) -> ComponentResult<ComponentActionClient<'entity, A>> {
        let metadata = entity_metadata(
            id,
            self.id,
            EntityKind::ActionClient,
            action_name,
            A::ACTION_NAME,
            A::ACTION_HASH,
            QosSettings::default(),
        )?;
        self.runtime.create_entity(metadata)?;
        Ok(ComponentActionClient::new(id))
    }

    /// Declare a parameter. Stable parameter ID is required.
    pub fn declare_parameter<'entity>(
        &mut self,
        id: EntityId<'entity>,
        name: &str,
        parameter_type: ParameterType,
    ) -> ComponentResult<ComponentParameter<'entity>> {
        let mut metadata = entity_metadata(
            id,
            self.id,
            EntityKind::Parameter,
            name,
            "",
            "",
            QosSettings::default(),
        )?;
        metadata.parameter_type = Some(parameter_type);
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
}
