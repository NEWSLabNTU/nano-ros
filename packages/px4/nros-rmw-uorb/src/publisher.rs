//! [`UorbPublisher`] implements [`nros_rmw::Publisher`].
//!
//! Phase 90.2 baseline: holds the resolved [`TopicEntry`]; `publish_raw`
//! returns `Backend("uORB: typed publish not wired")` until Phase 90.6 wires
//! the typed `px4_uorb::Publication<T>` codegen path.

use nros_rmw::{Publisher, TransportError};

use crate::topics::TopicEntry;

/// Publisher handle for one ROS 2 topic.
///
/// Ownership note: holds the topic descriptor only. The actual
/// `px4_uorb::Publication<T>` is constructed lazily on first publish, keyed
/// by the message type. This deferral is needed because the RMW trait
/// surface is type-erased (`publish_raw(&[u8])`) while uORB requires a
/// typed `Publication<T: UorbTopic>`.
#[derive(Debug)]
pub struct UorbPublisher {
    entry: TopicEntry,
}

impl UorbPublisher {
    pub(crate) fn new(entry: TopicEntry) -> Result<Self, TransportError> {
        Ok(Self { entry })
    }

    /// uORB topic name (e.g. `"sensor_gyro"`) this publisher writes to.
    pub fn uorb_name(&self) -> &'static str {
        self.entry.uorb_name
    }

    /// Multi-instance index.
    pub fn instance(&self) -> u8 {
        self.entry.instance
    }
}

impl Publisher for UorbPublisher {
    type Error = TransportError;

    fn publish_raw(&self, _data: &[u8]) -> Result<(), Self::Error> {
        // Phase 90.6 wires this through px4_uorb::Publication<T>::publish().
        // The blocker: type-erased &[u8] vs typed Publication<T>. Solution
        // sketch: emit a per-topic registry at codegen time that maps
        // (topic, type_hash) → fn(&[u8]) -> Result trampoline. See
        // docs/design/px4-rmw-uorb.md.
        Err(TransportError::Backend("uORB: typed publish not yet wired"))
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}
