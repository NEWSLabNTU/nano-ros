//! [`UorbPublisher`] implements [`nros_rmw::Publisher`].
//!
//! Phase 90.2 baseline: holds the resolved [`TopicEntry`]; `publish_raw`
//! returns `Backend("uORB: typed publish not wired")` until Phase 90.6 wires
//! the typed `px4_uorb::Publication<T>` codegen path.

use nros_rmw::{Publisher, TransportError};

use crate::registry::lookup;
use crate::topics::TopicEntry;

/// Publisher handle for one ROS 2 topic.
///
/// Holds the topic descriptor + a copy of the ROS 2 topic name (for
/// registry lookup at publish time). The typed `px4_uorb::Publication<T>`
/// lives in the [`crate::register`]-populated trampoline registry; this
/// type only stores the lookup key.
#[derive(Debug)]
pub struct UorbPublisher {
    entry: TopicEntry,
    ros_name: heapless::String<128>,
}

impl UorbPublisher {
    pub(crate) fn new(entry: TopicEntry, ros_name: &str) -> Result<Self, TransportError> {
        let mut buf = heapless::String::new();
        buf.push_str(ros_name)
            .map_err(|_| TransportError::InvalidConfig)?;
        Ok(Self {
            entry,
            ros_name: buf,
        })
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

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error> {
        let guard = lookup(self.ros_name.as_str()).ok_or(TransportError::Backend(
            "uORB: topic not registered — call nros_rmw_uorb::register::<T>(...) first",
        ))?;
        guard.handle().publish(data)
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}
