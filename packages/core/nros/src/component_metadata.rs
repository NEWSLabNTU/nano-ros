//! Component source metadata recorded without opening middleware.

use crate::{
    ParameterType, QosSettings,
    heapless::{String, Vec},
};

#[cfg(feature = "std")]
use crate::{QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy};
#[cfg(feature = "std")]
use std::{format, string::String as StdString, vec::Vec as StdVec};

/// Maximum nodes recorded by the built-in metadata recorder.
pub const DEFAULT_MAX_METADATA_NODES: usize = 8;
/// Maximum entities recorded by the built-in metadata recorder.
pub const DEFAULT_MAX_METADATA_ENTITIES: usize = 32;
/// Maximum callback/effect records kept by the built-in metadata recorder.
pub const DEFAULT_MAX_METADATA_CALLBACKS: usize = 32;
/// Maximum bytes in recorded source names and stable IDs.
pub const METADATA_STRING_CAPACITY: usize = 128;

/// Fixed-capacity string used by component metadata records.
pub type MetadataString = String<METADATA_STRING_CAPACITY>;

/// Unresolved ROS name category as written by component source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceNameKind {
    /// Starts with `/`.
    Absolute,
    /// Starts with `~`.
    Private,
    /// Any other non-empty source name.
    Relative,
}

impl SourceNameKind {
    /// Classify a source name without resolving launch remaps or namespaces.
    pub const fn from_source_name(name: &str) -> Self {
        let bytes = name.as_bytes();
        if bytes.is_empty() {
            Self::Relative
        } else if bytes[0] == b'/' {
            Self::Absolute
        } else if bytes[0] == b'~' {
            Self::Private
        } else {
            Self::Relative
        }
    }
}

/// Stable source-level identifier required for component-mode declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId<'a>(pub &'a str);

impl<'a> EntityId<'a> {
    /// Create a stable entity identifier.
    pub const fn new(id: &'a str) -> Self {
        Self(id)
    }

    /// Borrow the identifier string.
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// Stable node identifier required for component-mode node declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId<'a>(pub &'a str);

impl<'a> NodeId<'a> {
    /// Create a stable node identifier.
    pub const fn new(id: &'a str) -> Self {
        Self(id)
    }

    /// Borrow the identifier string.
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// Stable callback identifier required for component-mode callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackId<'a>(pub &'a str);

impl<'a> CallbackId<'a> {
    /// Create a stable callback identifier.
    pub const fn new(id: &'a str) -> Self {
        Self(id)
    }

    /// Borrow the identifier string.
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

/// Entity role recorded for source metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    Publisher,
    Subscription,
    Timer,
    ServiceServer,
    ServiceClient,
    ActionServer,
    ActionClient,
    Parameter,
}

/// Optional callback effect relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackEffectKind {
    Reads,
    Publishes,
    Writes,
}

/// Metadata recorder/runtime error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentMetadataError {
    /// Fixed recorder capacity exhausted.
    Capacity,
    /// Stable ID, node name, namespace, topic, service, action, or parameter name was too long.
    NameTooLong,
    /// Entity references a node ID that has not been declared.
    UnknownNode,
    /// Callback effect references an entity ID that has not been declared.
    UnknownEntity,
    /// Stable ID already exists in the same component metadata.
    DuplicateId,
}

/// Recorded node declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMetadata {
    pub id: MetadataString,
    pub name: MetadataString,
    pub namespace: MetadataString,
    pub domain_id: u32,
}

/// Recorded entity declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    pub id: MetadataString,
    pub node_id: MetadataString,
    pub kind: EntityKind,
    pub source_name: MetadataString,
    pub source_name_kind: SourceNameKind,
    pub type_name: &'static str,
    pub type_hash: &'static str,
    pub qos: QosSettings,
    pub callback_id: Option<MetadataString>,
    pub period_ms: Option<u64>,
    pub parameter_type: Option<ParameterType>,
}

/// Recorded optional callback effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackEffectMetadata {
    pub callback_id: MetadataString,
    pub kind: CallbackEffectKind,
    pub entity_id: MetadataString,
}

/// Source metadata document settings used by the std JSON emitter.
#[cfg(feature = "std")]
#[derive(Debug, Clone)]
pub struct SourceMetadataExport<'a> {
    pub package: &'a str,
    pub component: &'a str,
    pub executable: Option<&'a str>,
    pub exported_symbol: Option<&'a str>,
    pub package_manifest: &'a str,
    pub source_artifacts: &'a [&'a str],
}

#[cfg(feature = "std")]
impl<'a> SourceMetadataExport<'a> {
    /// Create export settings with ROS package and component names.
    pub const fn new(package: &'a str, component: &'a str) -> Self {
        Self {
            package,
            component,
            executable: None,
            exported_symbol: None,
            package_manifest: "package.xml",
            source_artifacts: &[],
        }
    }

    /// Set executable name.
    pub const fn executable(mut self, executable: &'a str) -> Self {
        self.executable = Some(executable);
        self
    }

    /// Set exported symbol name.
    pub const fn exported_symbol(mut self, exported_symbol: &'a str) -> Self {
        self.exported_symbol = Some(exported_symbol);
        self
    }

    /// Set package manifest path.
    pub const fn package_manifest(mut self, package_manifest: &'a str) -> Self {
        self.package_manifest = package_manifest;
        self
    }

    /// Set source artifact paths.
    pub const fn source_artifacts(mut self, source_artifacts: &'a [&'a str]) -> Self {
        self.source_artifacts = source_artifacts;
        self
    }
}

pub(crate) fn copy_str(value: &str) -> Result<MetadataString, ComponentMetadataError> {
    let mut out = MetadataString::new();
    out.push_str(value)
        .map_err(|_| ComponentMetadataError::NameTooLong)?;
    Ok(out)
}

/// In-memory metadata sink used by host discovery. It never opens transport.
#[derive(Debug)]
pub struct MetadataRecorder<
    const MAX_NODES: usize = DEFAULT_MAX_METADATA_NODES,
    const MAX_ENTITIES: usize = DEFAULT_MAX_METADATA_ENTITIES,
    const MAX_CALLBACKS: usize = DEFAULT_MAX_METADATA_CALLBACKS,
> {
    nodes: Vec<NodeMetadata, MAX_NODES>,
    entities: Vec<EntityMetadata, MAX_ENTITIES>,
    callback_effects: Vec<CallbackEffectMetadata, MAX_CALLBACKS>,
}

impl<const MAX_NODES: usize, const MAX_ENTITIES: usize, const MAX_CALLBACKS: usize> Default
    for MetadataRecorder<MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const MAX_NODES: usize, const MAX_ENTITIES: usize, const MAX_CALLBACKS: usize>
    MetadataRecorder<MAX_NODES, MAX_ENTITIES, MAX_CALLBACKS>
{
    /// Create an empty metadata recorder.
    pub const fn new() -> Self {
        Self {
            nodes: Vec::new(),
            entities: Vec::new(),
            callback_effects: Vec::new(),
        }
    }

    /// Recorded nodes in declaration order.
    pub fn nodes(&self) -> &[NodeMetadata] {
        &self.nodes
    }

    /// Recorded entities in declaration order.
    pub fn entities(&self) -> &[EntityMetadata] {
        &self.entities
    }

    /// Recorded optional callback effects in declaration order.
    pub fn callback_effects(&self) -> &[CallbackEffectMetadata] {
        &self.callback_effects
    }

    /// Emit schema-version-1 source metadata JSON without opening transport.
    #[cfg(feature = "std")]
    pub fn to_source_metadata_json(
        &self,
        export: &SourceMetadataExport<'_>,
    ) -> Result<StdString, core::fmt::Error> {
        let mut out = StdString::new();
        self.write_source_metadata_json(export, &mut out)?;
        Ok(out)
    }

    /// Write schema-version-1 source metadata JSON without opening transport.
    #[cfg(feature = "std")]
    pub fn write_source_metadata_json(
        &self,
        export: &SourceMetadataExport<'_>,
        out: &mut impl core::fmt::Write,
    ) -> core::fmt::Result {
        write!(out, "{{")?;
        write!(out, "\"version\":1,")?;
        write_json_field(out, "package", export.package)?;
        out.write_char(',')?;
        write_json_field(out, "component", export.component)?;
        out.write_char(',')?;
        write!(out, "\"language\":\"rust\",")?;
        write_json_opt_field(out, "executable", export.executable)?;
        out.write_char(',')?;
        write_json_opt_field(out, "exported_symbol", export.exported_symbol)?;
        out.write_char(',')?;
        self.write_nodes_json(out)?;
        out.write_char(',')?;
        self.write_callbacks_json(out)?;
        out.write_char(',')?;
        self.write_parameters_json(out)?;
        out.write_char(',')?;
        self.write_trace_json(export, out)?;
        write!(out, "}}")
    }

    pub(crate) fn push_node(
        &mut self,
        id: NodeId<'_>,
        name: &str,
        namespace: &str,
        domain_id: u32,
    ) -> Result<(), ComponentMetadataError> {
        if self.has_node(id.as_str()) {
            return Err(ComponentMetadataError::DuplicateId);
        }

        self.nodes
            .push(NodeMetadata {
                id: copy_str(id.as_str())?,
                name: copy_str(name)?,
                namespace: copy_str(namespace)?,
                domain_id,
            })
            .map_err(|_| ComponentMetadataError::Capacity)
    }

    pub(crate) fn push_entity(
        &mut self,
        entity: EntityMetadata,
    ) -> Result<(), ComponentMetadataError> {
        if !self.has_node(&entity.node_id) {
            return Err(ComponentMetadataError::UnknownNode);
        }
        if self.has_entity(&entity.id) {
            return Err(ComponentMetadataError::DuplicateId);
        }

        self.entities
            .push(entity)
            .map_err(|_| ComponentMetadataError::Capacity)
    }

    pub(crate) fn push_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> Result<(), ComponentMetadataError> {
        if !self.has_entity(entity_id.as_str()) {
            return Err(ComponentMetadataError::UnknownEntity);
        }

        self.callback_effects
            .push(CallbackEffectMetadata {
                callback_id: copy_str(callback_id.as_str())?,
                kind,
                entity_id: copy_str(entity_id.as_str())?,
            })
            .map_err(|_| ComponentMetadataError::Capacity)
    }

    pub(crate) fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id.as_str() == id)
    }

    pub(crate) fn has_entity(&self, id: &str) -> bool {
        self.entities.iter().any(|entity| entity.id.as_str() == id)
    }

    #[cfg(feature = "std")]
    fn write_nodes_json(&self, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(out, "\"nodes\":[")?;
        for (index, node) in self.nodes.iter().enumerate() {
            if index > 0 {
                out.write_char(',')?;
            }
            write!(out, "{{")?;
            write_json_field(out, "id", node.id.as_str())?;
            out.write_char(',')?;
            write!(out, "\"unresolved_name\":")?;
            write_source_name(
                out,
                node.name.as_str(),
                SourceNameKind::from_source_name(&node.name),
            )?;
            out.write_char(',')?;
            if node.namespace.as_str() == "/" {
                write!(out, "\"namespace\":null,")?;
            } else {
                write_json_field(out, "namespace", node.namespace.as_str())?;
                out.write_char(',')?;
            }
            self.write_node_entities(out, node.id.as_str())?;
            write!(out, "}}")?;
        }
        write!(out, "]")
    }

    #[cfg(feature = "std")]
    fn write_node_entities(
        &self,
        out: &mut impl core::fmt::Write,
        node_id: &str,
    ) -> core::fmt::Result {
        self.write_entity_array(out, "publishers", node_id, EntityKind::Publisher)?;
        out.write_char(',')?;
        self.write_entity_array(out, "subscribers", node_id, EntityKind::Subscription)?;
        out.write_char(',')?;
        self.write_entity_array(out, "timers", node_id, EntityKind::Timer)?;
        out.write_char(',')?;
        self.write_entity_array(out, "services", node_id, EntityKind::ServiceServer)?;
        out.write_char(',')?;
        self.write_entity_array(out, "actions", node_id, EntityKind::ActionServer)
    }

    #[cfg(feature = "std")]
    fn write_entity_array(
        &self,
        out: &mut impl core::fmt::Write,
        field: &str,
        node_id: &str,
        kind: EntityKind,
    ) -> core::fmt::Result {
        write!(out, "\"{}\":[", field)?;
        let mut emitted = 0usize;
        for entity in self
            .entities
            .iter()
            .filter(|entity| entity.node_id.as_str() == node_id && entity.kind == kind)
        {
            if emitted > 0 {
                out.write_char(',')?;
            }
            emitted += 1;
            match kind {
                EntityKind::Publisher => write_publisher_json(out, entity)?,
                EntityKind::Subscription => write_subscriber_json(out, entity)?,
                EntityKind::Timer => write_timer_json(out, entity)?,
                EntityKind::ServiceServer => write_service_json(out, entity)?,
                EntityKind::ActionServer => write_action_json(out, entity)?,
                _ => {}
            }
        }
        write!(out, "]")
    }

    #[cfg(feature = "std")]
    fn write_callbacks_json(&self, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        let callbacks = self.source_callbacks();
        write!(out, "\"callbacks\":[")?;
        for (index, callback) in callbacks.iter().enumerate() {
            if index > 0 {
                out.write_char(',')?;
            }
            write!(out, "{{")?;
            write_json_field(out, "id", callback.id.as_str())?;
            out.write_char(',')?;
            write_json_field(out, "kind", callback.kind)?;
            out.write_char(',')?;
            write!(out, "\"group\":null,")?;
            write!(out, "\"effects\":[")?;
            for (effect_index, effect) in self
                .callback_effects
                .iter()
                .filter(|effect| effect.callback_id.as_str() == callback.id)
                .enumerate()
            {
                if effect_index > 0 {
                    out.write_char(',')?;
                }
                write!(out, "{{")?;
                write_json_field(out, "kind", effect_json_kind(effect.kind))?;
                out.write_char(',')?;
                write_json_field(out, "entity", effect.entity_id.as_str())?;
                write!(out, "}}")?;
            }
            write!(out, "],")?;
            write_empty_source(out)?;
            write!(out, "}}")?;
        }
        write!(out, "]")
    }

    #[cfg(feature = "std")]
    fn write_parameters_json(&self, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(out, "\"parameters\":[")?;
        let mut emitted = 0usize;
        for entity in self
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Parameter)
        {
            if emitted > 0 {
                out.write_char(',')?;
            }
            emitted += 1;
            write!(out, "{{")?;
            write_json_field(out, "node", entity.node_id.as_str())?;
            out.write_char(',')?;
            write_json_field(out, "name", entity.source_name.as_str())?;
            out.write_char(',')?;
            write!(out, "\"default\":")?;
            write_parameter_default(out, entity.parameter_type.unwrap_or_default())?;
            out.write_char(',')?;
            write!(out, "\"read_only\":false,")?;
            write_empty_source(out)?;
            write!(out, "}}")?;
        }
        write!(out, "]")
    }

    #[cfg(feature = "std")]
    fn write_trace_json(
        &self,
        export: &SourceMetadataExport<'_>,
        out: &mut impl core::fmt::Write,
    ) -> core::fmt::Result {
        write!(out, "\"trace\":{{")?;
        write_json_field(out, "generator", "nros-metadata-rust")?;
        out.write_char(',')?;
        write_json_field(out, "package_manifest", export.package_manifest)?;
        out.write_char(',')?;
        write!(out, "\"source_artifacts\":[")?;
        for (index, artifact) in export.source_artifacts.iter().enumerate() {
            if index > 0 {
                out.write_char(',')?;
            }
            write_json_string(out, artifact)?;
        }
        write!(out, "]}}")
    }

    #[cfg(feature = "std")]
    fn source_callbacks(&self) -> StdVec<SourceCallbackRef> {
        let mut callbacks = StdVec::new();
        for entity in &self.entities {
            let Some(callback_id) = entity.callback_id.as_ref() else {
                continue;
            };
            let kind = match entity.kind {
                EntityKind::Subscription => "subscription",
                EntityKind::Timer => "timer",
                EntityKind::ServiceServer => "service",
                EntityKind::ActionServer => "action_goal",
                _ => continue,
            };
            if !callbacks
                .iter()
                .any(|callback: &SourceCallbackRef| callback.id == callback_id.as_str())
            {
                callbacks.push(SourceCallbackRef {
                    id: callback_id.as_str().into(),
                    kind,
                });
            }
        }
        callbacks
    }
}

#[cfg(feature = "std")]
struct SourceCallbackRef {
    id: StdString,
    kind: &'static str,
}

pub(crate) fn entity_metadata(
    id: EntityId<'_>,
    node_id: NodeId<'_>,
    kind: EntityKind,
    source_name: &str,
    type_name: &'static str,
    type_hash: &'static str,
    qos: QosSettings,
) -> Result<EntityMetadata, ComponentMetadataError> {
    Ok(EntityMetadata {
        id: copy_str(id.as_str())?,
        node_id: copy_str(node_id.as_str())?,
        kind,
        source_name: copy_str(source_name)?,
        source_name_kind: SourceNameKind::from_source_name(source_name),
        type_name,
        type_hash,
        qos,
        callback_id: None,
        period_ms: None,
        parameter_type: None,
    })
}

#[cfg(feature = "std")]
fn write_publisher_json(
    out: &mut impl core::fmt::Write,
    entity: &EntityMetadata,
) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    write!(out, "\"unresolved_topic\":")?;
    write_source_name(out, entity.source_name.as_str(), entity.source_name_kind)?;
    out.write_char(',')?;
    write_interface(out, entity.type_name, "message")?;
    out.write_char(',')?;
    write_qos(out, entity.qos)?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_subscriber_json(
    out: &mut impl core::fmt::Write,
    entity: &EntityMetadata,
) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    write!(out, "\"unresolved_topic\":")?;
    write_source_name(out, entity.source_name.as_str(), entity.source_name_kind)?;
    out.write_char(',')?;
    write_interface(out, entity.type_name, "message")?;
    out.write_char(',')?;
    write_qos(out, entity.qos)?;
    out.write_char(',')?;
    write_json_field(
        out,
        "callback",
        entity
            .callback_id
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or(""),
    )?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_timer_json(out: &mut impl core::fmt::Write, entity: &EntityMetadata) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    write!(out, "\"period_ms\":{},", entity.period_ms.unwrap_or(0))?;
    write_json_field(
        out,
        "callback",
        entity
            .callback_id
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or(""),
    )?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_service_json(
    out: &mut impl core::fmt::Write,
    entity: &EntityMetadata,
) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    write!(out, "\"unresolved_name\":")?;
    write_source_name(out, entity.source_name.as_str(), entity.source_name_kind)?;
    out.write_char(',')?;
    write_interface(out, entity.type_name, "service")?;
    out.write_char(',')?;
    write_json_field(
        out,
        "callback",
        entity
            .callback_id
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or(""),
    )?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_action_json(
    out: &mut impl core::fmt::Write,
    entity: &EntityMetadata,
) -> core::fmt::Result {
    let callback = entity
        .callback_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or("");
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    write!(out, "\"unresolved_name\":")?;
    write_source_name(out, entity.source_name.as_str(), entity.source_name_kind)?;
    out.write_char(',')?;
    write_interface(out, entity.type_name, "action")?;
    out.write_char(',')?;
    write_json_field(out, "goal_callback", callback)?;
    out.write_char(',')?;
    write_json_field(out, "cancel_callback", callback)?;
    out.write_char(',')?;
    write_json_field(out, "accepted_callback", callback)?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_source_name(
    out: &mut impl core::fmt::Write,
    value: &str,
    kind: SourceNameKind,
) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "value", value)?;
    out.write_char(',')?;
    write_json_field(out, "kind", source_name_kind_json(kind))?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_interface(
    out: &mut impl core::fmt::Write,
    type_name: &str,
    fallback_kind: &'static str,
) -> core::fmt::Result {
    let interface = parse_interface(type_name, fallback_kind);
    write!(out, "\"interface\":{{")?;
    write_json_field(out, "package", &interface.package)?;
    out.write_char(',')?;
    write_json_field(out, "name", &interface.name)?;
    out.write_char(',')?;
    write_json_field(out, "kind", interface.kind)?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_qos(out: &mut impl core::fmt::Write, qos: QosSettings) -> core::fmt::Result {
    write!(out, "\"qos\":{{")?;
    write_json_field(out, "reliability", reliability_json(qos.reliability))?;
    out.write_char(',')?;
    write_json_field(out, "durability", durability_json(qos.durability))?;
    out.write_char(',')?;
    write_json_field(out, "history", history_json(qos.history))?;
    out.write_char(',')?;
    write!(out, "\"depth\":{},", qos.depth)?;
    write_optional_ms(out, "deadline_ms", qos.deadline_ms)?;
    out.write_char(',')?;
    write_optional_ms(out, "lifespan_ms", qos.lifespan_ms)?;
    out.write_char(',')?;
    write_json_field(out, "liveliness", liveliness_json(qos.liveliness_kind))?;
    out.write_char(',')?;
    write_optional_ms(out, "liveliness_lease_duration_ms", qos.liveliness_lease_ms)?;
    write!(out, ",\"extensions\":{{}}}}")
}

#[cfg(feature = "std")]
fn write_empty_source(out: &mut impl core::fmt::Write) -> core::fmt::Result {
    write!(
        out,
        "\"source\":{{\"artifact\":\"\",\"line\":null,\"column\":null}}"
    )
}

#[cfg(feature = "std")]
fn write_parameter_default(
    out: &mut impl core::fmt::Write,
    param_type: ParameterType,
) -> core::fmt::Result {
    match param_type {
        ParameterType::Bool => write!(out, "false"),
        ParameterType::Integer => write!(out, "0"),
        ParameterType::Double => write!(out, "0.0"),
        ParameterType::String => write!(out, "\"\""),
        ParameterType::BoolArray => write!(out, "[]"),
        ParameterType::IntegerArray => write!(out, "[]"),
        ParameterType::DoubleArray => write!(out, "[]"),
        ParameterType::StringArray => write!(out, "[]"),
        ParameterType::ByteArray | ParameterType::NotSet => write!(out, "0"),
    }
}

#[cfg(feature = "std")]
fn write_json_field(out: &mut impl core::fmt::Write, name: &str, value: &str) -> core::fmt::Result {
    write_json_string(out, name)?;
    out.write_char(':')?;
    write_json_string(out, value)
}

#[cfg(feature = "std")]
fn write_json_opt_field(
    out: &mut impl core::fmt::Write,
    name: &str,
    value: Option<&str>,
) -> core::fmt::Result {
    write_json_string(out, name)?;
    out.write_char(':')?;
    if let Some(value) = value {
        write_json_string(out, value)
    } else {
        write!(out, "null")
    }
}

#[cfg(feature = "std")]
fn write_json_string(out: &mut impl core::fmt::Write, value: &str) -> core::fmt::Result {
    out.write_char('"')?;
    for ch in value.chars() {
        match ch {
            '"' => write!(out, "\\\"")?,
            '\\' => write!(out, "\\\\")?,
            '\n' => write!(out, "\\n")?,
            '\r' => write!(out, "\\r")?,
            '\t' => write!(out, "\\t")?,
            ch if ch.is_control() => write!(out, "\\u{:04x}", ch as u32)?,
            ch => out.write_char(ch)?,
        }
    }
    out.write_char('"')
}

#[cfg(feature = "std")]
fn write_optional_ms(out: &mut impl core::fmt::Write, name: &str, value: u32) -> core::fmt::Result {
    write_json_string(out, name)?;
    out.write_char(':')?;
    if value == 0 {
        write!(out, "null")
    } else {
        write!(out, "{}", value)
    }
}

#[cfg(feature = "std")]
fn source_name_kind_json(kind: SourceNameKind) -> &'static str {
    match kind {
        SourceNameKind::Absolute => "absolute",
        SourceNameKind::Relative => "relative",
        SourceNameKind::Private => "private",
    }
}

#[cfg(feature = "std")]
fn effect_json_kind(kind: CallbackEffectKind) -> &'static str {
    match kind {
        CallbackEffectKind::Publishes => "publishes",
        CallbackEffectKind::Reads => "reads_parameter",
        CallbackEffectKind::Writes => "writes_parameter",
    }
}

#[cfg(feature = "std")]
fn reliability_json(value: QosReliabilityPolicy) -> &'static str {
    match value {
        QosReliabilityPolicy::Reliable => "reliable",
        QosReliabilityPolicy::BestEffort => "best_effort",
    }
}

#[cfg(feature = "std")]
fn durability_json(value: QosDurabilityPolicy) -> &'static str {
    match value {
        QosDurabilityPolicy::Volatile => "volatile",
        QosDurabilityPolicy::TransientLocal => "transient_local",
    }
}

#[cfg(feature = "std")]
fn history_json(value: QosHistoryPolicy) -> &'static str {
    match value {
        QosHistoryPolicy::KeepLast => "keep_last",
        QosHistoryPolicy::KeepAll => "keep_all",
    }
}

#[cfg(feature = "std")]
fn liveliness_json(value: QosLivelinessPolicy) -> &'static str {
    match value {
        QosLivelinessPolicy::None => "system_default",
        QosLivelinessPolicy::Automatic => "automatic",
        QosLivelinessPolicy::ManualByTopic => "manual_by_topic",
        QosLivelinessPolicy::ManualByNode => "manual_by_topic",
    }
}

#[cfg(feature = "std")]
struct ParsedInterface {
    package: StdString,
    name: StdString,
    kind: &'static str,
}

#[cfg(feature = "std")]
fn parse_interface(type_name: &str, fallback_kind: &'static str) -> ParsedInterface {
    let parts: StdVec<&str> = type_name.split("::").collect();
    if parts.len() >= 4 {
        let package = parts[0].into();
        let kind = match parts[1] {
            "msg" => "message",
            "srv" => "service",
            "action" => "action",
            _ => fallback_kind,
        };
        let mut type_leaf = parts[3].trim_end_matches('_');
        if type_leaf.is_empty() {
            type_leaf = parts.last().copied().unwrap_or("");
        }
        return ParsedInterface {
            package,
            name: format!("{}/{}", parts[1], type_leaf),
            kind,
        };
    }

    ParsedInterface {
        package: StdString::new(),
        name: type_name.into(),
        kind: fallback_kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qos;

    #[test]
    fn source_name_kind_preserves_unresolved_names() {
        assert_eq!(
            SourceNameKind::from_source_name("/scan"),
            SourceNameKind::Absolute
        );
        assert_eq!(
            SourceNameKind::from_source_name("~/scan"),
            SourceNameKind::Private
        );
        assert_eq!(
            SourceNameKind::from_source_name("scan"),
            SourceNameKind::Relative
        );
    }

    #[test]
    fn recorder_rejects_duplicate_stable_ids() {
        let mut recorder = MetadataRecorder::<1, 2, 1>::new();
        recorder
            .push_node(NodeId::new("node"), "talker", "/", 0)
            .unwrap();

        let first = entity_metadata(
            EntityId::new("pub"),
            NodeId::new("node"),
            EntityKind::Publisher,
            "chatter",
            "std_msgs::msg::dds_::String_",
            "hash",
            qos::DEFAULT,
        )
        .unwrap();
        recorder.push_entity(first.clone()).unwrap();

        assert_eq!(
            recorder.push_entity(first),
            Err(ComponentMetadataError::DuplicateId)
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn source_metadata_json_uses_agent_a_schema_shape() {
        let mut recorder = MetadataRecorder::<1, 3, 1>::new();
        recorder
            .push_node(NodeId::new("node_talker"), "talker", "/", 0)
            .unwrap();
        recorder
            .push_entity(
                entity_metadata(
                    EntityId::new("pub_chatter"),
                    NodeId::new("node_talker"),
                    EntityKind::Publisher,
                    "chatter",
                    "std_msgs::msg::dds_::String_",
                    "hash",
                    crate::qos::DEFAULT,
                )
                .unwrap(),
            )
            .unwrap();
        let mut timer = entity_metadata(
            EntityId::new("timer_publish"),
            NodeId::new("node_talker"),
            EntityKind::Timer,
            "",
            "",
            "",
            crate::qos::DEFAULT,
        )
        .unwrap();
        timer.callback_id = Some(copy_str("cb_timer").unwrap());
        timer.period_ms = Some(100);
        recorder.push_entity(timer).unwrap();
        recorder
            .push_callback_effect(
                CallbackId::new("cb_timer"),
                CallbackEffectKind::Publishes,
                EntityId::new("pub_chatter"),
            )
            .unwrap();

        let json = recorder
            .to_source_metadata_json(
                &SourceMetadataExport::new("demo_nodes_rs", "talker")
                    .executable("talker")
                    .exported_symbol("nros_component_talker")
                    .source_artifacts(&["src/talker.rs"]),
            )
            .unwrap();

        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"language\":\"rust\""));
        assert!(json.contains("\"unresolved_name\":{\"value\":\"talker\",\"kind\":\"relative\"}"));
        assert!(json.contains(
            "\"interface\":{\"package\":\"std_msgs\",\"name\":\"msg/String\",\"kind\":\"message\"}"
        ));
        assert!(json.contains("\"kind\":\"publishes\",\"entity\":\"pub_chatter\""));
        assert!(json.contains("\"generator\":\"nros-metadata-rust\""));
    }
}
