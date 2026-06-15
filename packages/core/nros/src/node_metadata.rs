//! Node source metadata recorded without opening middleware.

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

/// Declaration-order node slot within one extracted component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeSlot(pub usize);

impl NodeSlot {
    /// Create a node slot from its declaration-order index.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Declaration-order index.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Declaration-order entity slot within one extracted component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntitySlot(pub usize);

impl EntitySlot {
    /// Create an entity slot from its declaration-order index.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Declaration-order index.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Declaration-order callback slot within one extracted component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallbackSlot(pub usize);

impl CallbackSlot {
    /// Create a callback slot from its declaration-order index.
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Declaration-order index.
    pub const fn index(self) -> usize {
        self.0
    }
}

/// Source location attached to callbacks and parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocationMetadata {
    pub artifact: MetadataString,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

impl SourceLocationMetadata {
    /// Empty source location used when caller data is unavailable.
    pub const fn empty() -> Self {
        Self {
            artifact: MetadataString::new(),
            line: None,
            column: None,
        }
    }

    /// Capture the Rust caller location.
    #[track_caller]
    pub fn caller() -> Result<Self, NodeMetadataError> {
        let location = core::panic::Location::caller();
        Ok(Self {
            artifact: copy_str(location.file())?,
            line: Some(location.line()),
            column: Some(location.column()),
        })
    }
}

/// Parameter default value recorded for source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParameterDefault {
    Bool(bool),
    Integer(i64),
    Double(MetadataString),
    String(MetadataString),
    BoolArray,
    IntegerArray,
    DoubleArray,
    StringArray,
}

impl ParameterDefault {
    /// Parameter type implied by this default.
    pub const fn parameter_type(&self) -> ParameterType {
        match self {
            Self::Bool(_) => ParameterType::Bool,
            Self::Integer(_) => ParameterType::Integer,
            Self::Double(_) => ParameterType::Double,
            Self::String(_) => ParameterType::String,
            Self::BoolArray => ParameterType::BoolArray,
            Self::IntegerArray => ParameterType::IntegerArray,
            Self::DoubleArray => ParameterType::DoubleArray,
            Self::StringArray => ParameterType::StringArray,
        }
    }

    /// Default JSON-compatible value for a parameter type.
    pub fn for_type(param_type: ParameterType) -> Result<Self, NodeMetadataError> {
        Ok(match param_type {
            ParameterType::Bool => Self::Bool(false),
            ParameterType::Integer => Self::Integer(0),
            ParameterType::Double => Self::Double(copy_str("0.0")?),
            ParameterType::String => Self::String(copy_str("")?),
            ParameterType::BoolArray => Self::BoolArray,
            ParameterType::IntegerArray => Self::IntegerArray,
            ParameterType::DoubleArray => Self::DoubleArray,
            ParameterType::StringArray => Self::StringArray,
            ParameterType::ByteArray | ParameterType::NotSet => Self::Integer(0),
        })
    }
}

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
pub enum NodeMetadataError {
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
    pub slot: NodeSlot,
    pub id: MetadataString,
    pub source_default_name: MetadataString,
    pub name: MetadataString,
    pub namespace: MetadataString,
    pub domain_id: u32,
}

/// Recorded entity declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    pub slot: Option<EntitySlot>,
    pub id: MetadataString,
    pub node_slot: Option<NodeSlot>,
    pub node_id: MetadataString,
    pub kind: EntityKind,
    pub source_name: MetadataString,
    pub source_name_kind: SourceNameKind,
    pub type_name: &'static str,
    pub type_hash: &'static str,
    pub qos: QosSettings,
    pub callback_slot: Option<CallbackSlot>,
    pub callback_id: Option<MetadataString>,
    pub callback_source: SourceLocationMetadata,
    pub callback_group: Option<MetadataString>,
    pub action_cancel_callback_slot: Option<CallbackSlot>,
    pub action_cancel_callback_id: Option<MetadataString>,
    pub action_cancel_source: SourceLocationMetadata,
    pub action_accepted_callback_slot: Option<CallbackSlot>,
    pub action_accepted_callback_id: Option<MetadataString>,
    pub action_accepted_source: SourceLocationMetadata,
    pub period_ms: Option<u64>,
    pub parameter_type: Option<ParameterType>,
    pub parameter_default: Option<ParameterDefault>,
    pub parameter_read_only: bool,
    /// Phase 250 (Wave 2b) — a subscription that opted into E2E message-integrity
    /// validation (`.safety()`): the runtime registers it via
    /// `create_generic_subscription_with_integrity` and surfaces
    /// [`IntegrityStatus`](crate::IntegrityStatus) through `CallbackCtx::integrity()`.
    /// Ungated (a plain flag) — only the runtime branch that reads it is gated on
    /// `safety-e2e`, so when the capability is off the flag is simply ignored
    /// (the subscription registers as a basic one). `false` for every other entity.
    pub safety: bool,
    pub source: SourceLocationMetadata,
}

/// Recorded optional callback effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackEffectMetadata {
    pub callback_id: MetadataString,
    pub callback_slot: Option<CallbackSlot>,
    pub kind: CallbackEffectKind,
    pub entity_id: MetadataString,
    pub entity_slot: Option<EntitySlot>,
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

pub(crate) fn copy_str(value: &str) -> Result<MetadataString, NodeMetadataError> {
    let mut out = MetadataString::new();
    out.push_str(value)
        .map_err(|_| NodeMetadataError::NameTooLong)?;
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
    ) -> Result<(), NodeMetadataError> {
        if self.has_node(id.as_str()) {
            return Err(NodeMetadataError::DuplicateId);
        }

        self.nodes
            .push(NodeMetadata {
                slot: NodeSlot::new(self.nodes.len()),
                id: copy_str(id.as_str())?,
                source_default_name: copy_str(name)?,
                name: copy_str(name)?,
                namespace: copy_str(namespace)?,
                domain_id,
            })
            .map_err(|_| NodeMetadataError::Capacity)
    }

    pub(crate) fn push_entity(
        &mut self,
        mut entity: EntityMetadata,
    ) -> Result<(), NodeMetadataError> {
        if !self.has_node(&entity.node_id) {
            return Err(NodeMetadataError::UnknownNode);
        }
        if self.has_entity(&entity.id) {
            return Err(NodeMetadataError::DuplicateId);
        }

        entity.slot = Some(EntitySlot::new(self.entities.len()));
        entity.node_slot = self.node_slot_for_id(&entity.node_id);
        let mut current_callbacks = Vec::<MetadataString, 3>::new();
        let mut next_callback_slot = self.callback_slot_count();
        entity.callback_slot = entity.callback_id.as_ref().map(|callback_id| {
            self.callback_slot_for_current_entity(
                callback_id.as_str(),
                &mut current_callbacks,
                &mut next_callback_slot,
            )
        });
        entity.action_cancel_callback_slot =
            entity
                .action_cancel_callback_id
                .as_ref()
                .map(|callback_id| {
                    self.callback_slot_for_current_entity(
                        callback_id.as_str(),
                        &mut current_callbacks,
                        &mut next_callback_slot,
                    )
                });
        entity.action_accepted_callback_slot =
            entity
                .action_accepted_callback_id
                .as_ref()
                .map(|callback_id| {
                    self.callback_slot_for_current_entity(
                        callback_id.as_str(),
                        &mut current_callbacks,
                        &mut next_callback_slot,
                    )
                });

        self.entities
            .push(entity)
            .map_err(|_| NodeMetadataError::Capacity)
    }

    pub(crate) fn push_callback_effect(
        &mut self,
        callback_id: CallbackId<'_>,
        kind: CallbackEffectKind,
        entity_id: EntityId<'_>,
    ) -> Result<(), NodeMetadataError> {
        if !self.has_entity(entity_id.as_str()) {
            return Err(NodeMetadataError::UnknownEntity);
        }

        self.callback_effects
            .push(CallbackEffectMetadata {
                callback_id: copy_str(callback_id.as_str())?,
                callback_slot: self.callback_slot_for_id(callback_id.as_str()),
                kind,
                entity_id: copy_str(entity_id.as_str())?,
                entity_slot: self.entity_slot_for_id(entity_id.as_str()),
            })
            .map_err(|_| NodeMetadataError::Capacity)
    }

    pub(crate) fn has_node(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id.as_str() == id)
    }

    pub(crate) fn has_entity(&self, id: &str) -> bool {
        self.entities.iter().any(|entity| entity.id.as_str() == id)
    }

    fn node_slot_for_id(&self, id: &str) -> Option<NodeSlot> {
        self.nodes
            .iter()
            .find(|node| node.id.as_str() == id)
            .map(|node| node.slot)
    }

    fn entity_slot_for_id(&self, id: &str) -> Option<EntitySlot> {
        self.entities
            .iter()
            .find(|entity| entity.id.as_str() == id)
            .and_then(|entity| entity.slot)
    }

    fn callback_slot_for_current_entity(
        &self,
        id: &str,
        current_callbacks: &mut Vec<MetadataString, 3>,
        next_callback_slot: &mut usize,
    ) -> CallbackSlot {
        if let Some(slot) = self.callback_slot_for_id(id) {
            return slot;
        }
        if let Some((index, _)) = current_callbacks
            .iter()
            .enumerate()
            .find(|(_, callback_id)| callback_id.as_str() == id)
        {
            return CallbackSlot::new(self.callback_slot_count() + index);
        }
        let slot = CallbackSlot::new(*next_callback_slot);
        let _ = current_callbacks
            .push(copy_str(id).expect("callback ID already fits metadata string capacity"));
        *next_callback_slot += 1;
        slot
    }

    fn callback_slot_for_id(&self, id: &str) -> Option<CallbackSlot> {
        let mut seen = Vec::<&str, MAX_CALLBACKS>::new();
        for entity in &self.entities {
            for callback_id in entity_callback_ids(entity) {
                let Some(callback_id) = callback_id else {
                    continue;
                };
                let callback_id = callback_id.as_str();
                if seen.contains(&callback_id) {
                    continue;
                }
                if callback_id == id {
                    return Some(CallbackSlot::new(seen.len()));
                }
                let _ = seen.push(callback_id);
            }
        }
        None
    }

    fn callback_slot_count(&self) -> usize {
        let mut seen = Vec::<&str, MAX_CALLBACKS>::new();
        for entity in &self.entities {
            for callback_id in entity_callback_ids(entity) {
                let Some(callback_id) = callback_id else {
                    continue;
                };
                let callback_id = callback_id.as_str();
                if !seen.contains(&callback_id) {
                    let _ = seen.push(callback_id);
                }
            }
        }
        seen.len()
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
            write!(out, "\"declaration_slot\":{},", node.slot.index())?;
            write_json_field(
                out,
                "source_default_name",
                node.source_default_name.as_str(),
            )?;
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
        for (index, entity) in self
            .entities
            .iter()
            .filter(|entity| entity.node_id.as_str() == node_id && entity.kind == kind)
            .enumerate()
        {
            if index > 0 {
                out.write_char(',')?;
            }
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
            if let Some(slot) = callback.slot {
                write!(out, "\"declaration_slot\":{},", slot.index())?;
            }
            write_json_field(out, "kind", callback.kind)?;
            out.write_char(',')?;
            if let Some(group) = callback.group.as_ref() {
                write_json_field(out, "group", group)?;
                out.write_char(',')?;
            } else {
                write!(out, "\"group\":null,")?;
            }
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
                if let Some(entity_slot) = effect.entity_slot {
                    write!(out, ",\"entity_slot\":{}", entity_slot.index())?;
                }
                write!(out, "}}")?;
            }
            write!(out, "],")?;
            write_source_location(out, &callback.source)?;
            write!(out, "}}")?;
        }
        write!(out, "]")
    }

    #[cfg(feature = "std")]
    fn write_parameters_json(&self, out: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(out, "\"parameters\":[")?;
        for (index, entity) in self
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::Parameter)
            .enumerate()
        {
            if index > 0 {
                out.write_char(',')?;
            }
            write!(out, "{{")?;
            write_json_field(out, "node", entity.node_id.as_str())?;
            out.write_char(',')?;
            if let Some(slot) = entity.slot {
                write!(out, "\"declaration_slot\":{},", slot.index())?;
            }
            write_json_field(out, "name", entity.source_name.as_str())?;
            out.write_char(',')?;
            write!(out, "\"default\":")?;
            write_parameter_default(out, entity.parameter_default.as_ref())?;
            out.write_char(',')?;
            write!(out, "\"read_only\":{},", entity.parameter_read_only)?;
            write_source_location(out, &entity.source)?;
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
                    slot: entity.callback_slot,
                    kind,
                    source: entity.callback_source.clone(),
                    group: entity
                        .callback_group
                        .as_ref()
                        .map(|group| group.as_str().into()),
                });
            }
            if entity.kind == EntityKind::ActionServer {
                if let Some(cancel_id) = entity.action_cancel_callback_id.as_ref()
                    && !callbacks
                        .iter()
                        .any(|callback: &SourceCallbackRef| callback.id == cancel_id.as_str())
                {
                    callbacks.push(SourceCallbackRef {
                        id: cancel_id.as_str().into(),
                        slot: entity.action_cancel_callback_slot,
                        kind: "action_cancel",
                        source: entity.action_cancel_source.clone(),
                        group: entity
                            .callback_group
                            .as_ref()
                            .map(|group| group.as_str().into()),
                    });
                }
                if let Some(accepted_id) = entity.action_accepted_callback_id.as_ref()
                    && !callbacks
                        .iter()
                        .any(|callback: &SourceCallbackRef| callback.id == accepted_id.as_str())
                {
                    callbacks.push(SourceCallbackRef {
                        id: accepted_id.as_str().into(),
                        slot: entity.action_accepted_callback_slot,
                        kind: "action_accepted",
                        source: entity.action_accepted_source.clone(),
                        group: entity
                            .callback_group
                            .as_ref()
                            .map(|group| group.as_str().into()),
                    });
                }
            }
        }
        callbacks
    }
}

#[cfg(feature = "std")]
struct SourceCallbackRef {
    id: StdString,
    slot: Option<CallbackSlot>,
    kind: &'static str,
    source: SourceLocationMetadata,
    group: Option<StdString>,
}

pub(crate) fn entity_callback_ids(entity: &EntityMetadata) -> [Option<&MetadataString>; 3] {
    [
        entity.callback_id.as_ref(),
        entity.action_cancel_callback_id.as_ref(),
        entity.action_accepted_callback_id.as_ref(),
    ]
}

/// Inputs for [`entity_metadata`]. Collapses the seven positional
/// arguments — three of them adjacent `&str` (`source_name` /
/// `type_name` / `type_hash`) that are trivially transposable at a
/// call site — into one named-field struct.
pub(crate) struct EntityMetadataSpec<'a> {
    pub id: EntityId<'a>,
    pub node_id: NodeId<'a>,
    pub kind: EntityKind,
    pub source_name: &'a str,
    pub type_name: &'static str,
    pub type_hash: &'static str,
    pub qos: QosSettings,
}

pub(crate) fn entity_metadata(
    spec: EntityMetadataSpec<'_>,
) -> Result<EntityMetadata, NodeMetadataError> {
    let EntityMetadataSpec {
        id,
        node_id,
        kind,
        source_name,
        type_name,
        type_hash,
        qos,
    } = spec;
    Ok(EntityMetadata {
        slot: None,
        id: copy_str(id.as_str())?,
        node_slot: None,
        node_id: copy_str(node_id.as_str())?,
        kind,
        source_name: copy_str(source_name)?,
        source_name_kind: SourceNameKind::from_source_name(source_name),
        type_name,
        type_hash,
        qos,
        callback_slot: None,
        callback_id: None,
        callback_source: SourceLocationMetadata::empty(),
        callback_group: None,
        action_cancel_callback_slot: None,
        action_cancel_callback_id: None,
        action_cancel_source: SourceLocationMetadata::empty(),
        action_accepted_callback_slot: None,
        action_accepted_callback_id: None,
        action_accepted_source: SourceLocationMetadata::empty(),
        period_ms: None,
        parameter_type: None,
        parameter_default: None,
        parameter_read_only: false,
        safety: false,
        source: SourceLocationMetadata::empty(),
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
    if let Some(slot) = entity.slot {
        write!(out, "\"declaration_slot\":{},", slot.index())?;
    }
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
    if let Some(slot) = entity.slot {
        write!(out, "\"declaration_slot\":{},", slot.index())?;
    }
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
    if let Some(callback_slot) = entity.callback_slot {
        write!(out, ",\"callback_slot\":{}", callback_slot.index())?;
    }
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_timer_json(out: &mut impl core::fmt::Write, entity: &EntityMetadata) -> core::fmt::Result {
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    if let Some(slot) = entity.slot {
        write!(out, "\"declaration_slot\":{},", slot.index())?;
    }
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
    if let Some(callback_slot) = entity.callback_slot {
        write!(out, ",\"callback_slot\":{}", callback_slot.index())?;
    }
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
    if let Some(slot) = entity.slot {
        write!(out, "\"declaration_slot\":{},", slot.index())?;
    }
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
    if let Some(callback_slot) = entity.callback_slot {
        write!(out, ",\"callback_slot\":{}", callback_slot.index())?;
    }
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_action_json(
    out: &mut impl core::fmt::Write,
    entity: &EntityMetadata,
) -> core::fmt::Result {
    let goal_callback = entity
        .callback_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or("");
    let cancel_callback = entity
        .action_cancel_callback_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or(goal_callback);
    let accepted_callback = entity
        .action_accepted_callback_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or(goal_callback);
    write!(out, "{{")?;
    write_json_field(out, "id", entity.id.as_str())?;
    out.write_char(',')?;
    if let Some(slot) = entity.slot {
        write!(out, "\"declaration_slot\":{},", slot.index())?;
    }
    write!(out, "\"unresolved_name\":")?;
    write_source_name(out, entity.source_name.as_str(), entity.source_name_kind)?;
    out.write_char(',')?;
    write_interface(out, entity.type_name, "action")?;
    out.write_char(',')?;
    write_json_field(out, "goal_callback", goal_callback)?;
    if let Some(callback_slot) = entity.callback_slot {
        write!(out, ",\"goal_callback_slot\":{}", callback_slot.index())?;
    }
    out.write_char(',')?;
    write_json_field(out, "cancel_callback", cancel_callback)?;
    if let Some(callback_slot) = entity.action_cancel_callback_slot {
        write!(out, ",\"cancel_callback_slot\":{}", callback_slot.index())?;
    }
    out.write_char(',')?;
    write_json_field(out, "accepted_callback", accepted_callback)?;
    if let Some(callback_slot) = entity.action_accepted_callback_slot {
        write!(out, ",\"accepted_callback_slot\":{}", callback_slot.index())?;
    }
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
fn write_source_location(
    out: &mut impl core::fmt::Write,
    source: &SourceLocationMetadata,
) -> core::fmt::Result {
    write!(out, "\"source\":{{")?;
    write_json_field(out, "artifact", source.artifact.as_str())?;
    out.write_char(',')?;
    write!(out, "\"line\":")?;
    write_optional_u32(out, source.line)?;
    out.write_char(',')?;
    write!(out, "\"column\":")?;
    write_optional_u32(out, source.column)?;
    write!(out, "}}")
}

#[cfg(feature = "std")]
fn write_parameter_default(
    out: &mut impl core::fmt::Write,
    default: Option<&ParameterDefault>,
) -> core::fmt::Result {
    match default {
        Some(ParameterDefault::Bool(value)) => write!(out, "{}", value),
        Some(ParameterDefault::Integer(value)) => write!(out, "{}", value),
        Some(ParameterDefault::Double(value)) => write!(out, "{}", value.as_str()),
        Some(ParameterDefault::String(value)) => write_json_string(out, value.as_str()),
        Some(ParameterDefault::BoolArray)
        | Some(ParameterDefault::IntegerArray)
        | Some(ParameterDefault::DoubleArray)
        | Some(ParameterDefault::StringArray)
        | None => write!(out, "[]"),
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
fn write_optional_u32(out: &mut impl core::fmt::Write, value: Option<u32>) -> core::fmt::Result {
    if let Some(value) = value {
        write!(out, "{}", value)
    } else {
        write!(out, "null")
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

        let first = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("pub"),
            node_id: NodeId::new("node"),
            kind: EntityKind::Publisher,
            source_name: "chatter",
            type_name: "std_msgs::msg::dds_::String_",
            type_hash: "hash",
            qos: qos::DEFAULT,
        })
        .unwrap();
        recorder.push_entity(first.clone()).unwrap();

        assert_eq!(
            recorder.push_entity(first),
            Err(NodeMetadataError::DuplicateId)
        );
    }

    #[test]
    fn recorder_rejects_duplicate_nodes_and_unknown_node_entities() {
        let mut recorder = MetadataRecorder::<1, 1, 1>::new();
        recorder
            .push_node(NodeId::new("node"), "talker", "/", 0)
            .unwrap();

        assert_eq!(
            recorder.push_node(NodeId::new("node"), "other", "/", 0),
            Err(NodeMetadataError::DuplicateId)
        );

        let entity = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("pub"),
            node_id: NodeId::new("missing_node"),
            kind: EntityKind::Publisher,
            source_name: "chatter",
            type_name: "std_msgs::msg::dds_::String_",
            type_hash: "hash",
            qos: qos::DEFAULT,
        })
        .unwrap();

        assert_eq!(
            recorder.push_entity(entity),
            Err(NodeMetadataError::UnknownNode)
        );
    }

    #[test]
    fn recorder_assigns_slots_and_source_default_names_by_declaration_order() {
        let mut recorder = MetadataRecorder::<2, 4, 3>::new();
        recorder
            .push_node(NodeId::new("node_alpha"), "talker", "/", 0)
            .unwrap();
        recorder
            .push_node(NodeId::new("node_beta"), "listener", "/demo", 42)
            .unwrap();

        assert_eq!(recorder.nodes()[0].slot, NodeSlot::new(0));
        assert_eq!(recorder.nodes()[0].source_default_name.as_str(), "talker");
        assert_eq!(recorder.nodes()[1].slot, NodeSlot::new(1));
        assert_eq!(recorder.nodes()[1].source_default_name.as_str(), "listener");

        recorder
            .push_entity(
                entity_metadata(EntityMetadataSpec {
                    id: EntityId::new("pub_chatter"),
                    node_id: NodeId::new("node_alpha"),
                    kind: EntityKind::Publisher,
                    source_name: "/chatter",
                    type_name: "std_msgs::msg::dds_::String_",
                    type_hash: "hash",
                    qos: qos::DEFAULT,
                })
                .unwrap(),
            )
            .unwrap();
        let mut subscription = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("sub_chatter"),
            node_id: NodeId::new("node_beta"),
            kind: EntityKind::Subscription,
            source_name: "/chatter",
            type_name: "std_msgs::msg::dds_::String_",
            type_hash: "hash",
            qos: qos::DEFAULT,
        })
        .unwrap();
        subscription.callback_id = Some(copy_str("on_message").unwrap());
        recorder.push_entity(subscription).unwrap();
        let mut timer = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("timer_tick"),
            node_id: NodeId::new("node_alpha"),
            kind: EntityKind::Timer,
            source_name: "",
            type_name: "",
            type_hash: "",
            qos: qos::DEFAULT,
        })
        .unwrap();
        timer.callback_id = Some(copy_str("on_tick").unwrap());
        recorder.push_entity(timer).unwrap();

        assert_eq!(recorder.entities()[0].slot, Some(EntitySlot::new(0)));
        assert_eq!(recorder.entities()[0].node_slot, Some(NodeSlot::new(0)));
        assert_eq!(recorder.entities()[0].callback_slot, None);
        assert_eq!(recorder.entities()[1].slot, Some(EntitySlot::new(1)));
        assert_eq!(recorder.entities()[1].node_slot, Some(NodeSlot::new(1)));
        assert_eq!(
            recorder.entities()[1].callback_slot,
            Some(CallbackSlot::new(0))
        );
        assert_eq!(
            recorder.entities()[2].callback_slot,
            Some(CallbackSlot::new(1))
        );

        recorder
            .push_callback_effect(
                CallbackId::new("on_tick"),
                CallbackEffectKind::Publishes,
                EntityId::new("pub_chatter"),
            )
            .unwrap();
        assert_eq!(
            recorder.callback_effects()[0].callback_slot,
            Some(CallbackSlot::new(1))
        );
        assert_eq!(
            recorder.callback_effects()[0].entity_slot,
            Some(EntitySlot::new(0))
        );
    }

    #[test]
    fn recorder_assigns_distinct_callback_slots_within_one_action_entity() {
        let mut recorder = MetadataRecorder::<1, 1, 3>::new();
        recorder
            .push_node(NodeId::new("node"), "action_node", "/", 0)
            .unwrap();
        let mut action = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("act_count"),
            node_id: NodeId::new("node"),
            kind: EntityKind::ActionServer,
            source_name: "/count",
            type_name: "example_interfaces::action::dds_::Fibonacci_",
            type_hash: "hash",
            qos: qos::DEFAULT,
        })
        .unwrap();
        action.callback_id = Some(copy_str("on_goal").unwrap());
        action.action_cancel_callback_id = Some(copy_str("on_cancel").unwrap());
        action.action_accepted_callback_id = Some(copy_str("on_accepted").unwrap());

        recorder.push_entity(action).unwrap();

        assert_eq!(
            recorder.entities()[0].callback_slot,
            Some(CallbackSlot::new(0))
        );
        assert_eq!(
            recorder.entities()[0].action_cancel_callback_slot,
            Some(CallbackSlot::new(1))
        );
        assert_eq!(
            recorder.entities()[0].action_accepted_callback_slot,
            Some(CallbackSlot::new(2))
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn source_metadata_json_uses_agent_a_schema_shape() {
        let mut recorder = MetadataRecorder::<1, 5, 1>::new();
        recorder
            .push_node(NodeId::new("node_talker"), "talker", "/", 0)
            .unwrap();
        recorder
            .push_entity(
                entity_metadata(EntityMetadataSpec {
                    id: EntityId::new("pub_chatter"),
                    node_id: NodeId::new("node_talker"),
                    kind: EntityKind::Publisher,
                    source_name: "chatter",
                    type_name: "std_msgs::msg::dds_::String_",
                    type_hash: "hash",
                    qos: crate::qos::DEFAULT,
                })
                .unwrap(),
            )
            .unwrap();
        let mut timer = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("timer_publish"),
            node_id: NodeId::new("node_talker"),
            kind: EntityKind::Timer,
            source_name: "",
            type_name: "",
            type_hash: "",
            qos: crate::qos::DEFAULT,
        })
        .unwrap();
        timer.callback_id = Some(copy_str("cb_timer").unwrap());
        timer.callback_source = SourceLocationMetadata {
            artifact: copy_str("src/talker.rs").unwrap(),
            line: Some(42),
            column: Some(5),
        };
        timer.period_ms = Some(100);
        recorder.push_entity(timer).unwrap();
        let mut param = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("param_rate"),
            node_id: NodeId::new("node_talker"),
            kind: EntityKind::Parameter,
            source_name: "rate_hz",
            type_name: "",
            type_hash: "",
            qos: crate::qos::DEFAULT,
        })
        .unwrap();
        param.parameter_type = Some(ParameterType::Integer);
        param.parameter_default = Some(ParameterDefault::Integer(10));
        param.source = SourceLocationMetadata {
            artifact: copy_str("src/talker.rs").unwrap(),
            line: Some(25),
            column: Some(9),
        };
        recorder.push_entity(param).unwrap();
        let mut action = entity_metadata(EntityMetadataSpec {
            id: EntityId::new("act_count"),
            node_id: NodeId::new("node_talker"),
            kind: EntityKind::ActionServer,
            source_name: "~/count",
            type_name: "example_interfaces::action::dds_::Fibonacci_",
            type_hash: "hash",
            qos: crate::qos::DEFAULT,
        })
        .unwrap();
        action.callback_id = Some(copy_str("cb_count_goal").unwrap());
        action.callback_source = SourceLocationMetadata {
            artifact: copy_str("src/talker.rs").unwrap(),
            line: Some(90),
            column: Some(5),
        };
        action.action_cancel_callback_id = Some(copy_str("cb_count_cancel").unwrap());
        action.action_cancel_source = SourceLocationMetadata {
            artifact: copy_str("src/talker.rs").unwrap(),
            line: Some(96),
            column: Some(5),
        };
        action.action_accepted_callback_id = Some(copy_str("cb_count_accepted").unwrap());
        action.action_accepted_source = SourceLocationMetadata {
            artifact: copy_str("src/talker.rs").unwrap(),
            line: Some(104),
            column: Some(5),
        };
        recorder.push_entity(action).unwrap();
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
                    .exported_symbol("nros_node_talker")
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
        assert!(
            json.contains("\"source\":{\"artifact\":\"src/talker.rs\",\"line\":42,\"column\":5}")
        );
        assert!(json.contains("\"name\":\"rate_hz\",\"default\":10,\"read_only\":false"));
        assert!(json.contains("\"goal_callback\":\"cb_count_goal\""));
        assert!(json.contains("\"cancel_callback\":\"cb_count_cancel\""));
        assert!(json.contains("\"accepted_callback\":\"cb_count_accepted\""));
        assert!(json.contains("\"kind\":\"action_cancel\""));
        assert!(json.contains("\"kind\":\"action_accepted\""));
        assert!(json.contains("\"generator\":\"nros-metadata-rust\""));
    }
}
