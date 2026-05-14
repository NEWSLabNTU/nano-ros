//! Component source metadata recorded without opening middleware.

use crate::{
    ParameterType, QosSettings,
    heapless::{String, Vec},
};

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
}
