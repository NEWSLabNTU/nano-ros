//! [`UorbSubscriber`] implements [`nros_rmw::Subscriber`].
//!
//! Phase 90.2 baseline: holds the resolved [`TopicEntry`]; `try_recv_raw`
//! returns `Ok(None)` (no data) until Phase 90.6 wires the typed
//! `px4_uorb::Subscription<T>` codegen path.

use nros_rmw::{Subscriber, TransportError};

use crate::registry::lookup;
use crate::topics::TopicEntry;

/// Subscriber handle for one ROS 2 topic. Holds the topic descriptor and
/// ROS 2 name; the typed `px4_uorb::Subscription<T>` lives in the trampoline
/// registry populated by [`crate::register`].
#[derive(Debug)]
pub struct UorbSubscriber {
    entry: TopicEntry,
    ros_name: heapless::String<128>,
}

impl UorbSubscriber {
    pub(crate) fn new(entry: TopicEntry, ros_name: &str) -> Result<Self, TransportError> {
        let mut buf = heapless::String::new();
        buf.push_str(ros_name)
            .map_err(|_| TransportError::InvalidConfig)?;
        Ok(Self {
            entry,
            ros_name: buf,
        })
    }

    pub fn uorb_name(&self) -> &'static str {
        self.entry.uorb_name
    }

    pub fn instance(&self) -> u8 {
        self.entry.instance
    }
}

impl Subscriber for UorbSubscriber {
    type Error = TransportError;

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        let guard = lookup(self.ros_name.as_str()).ok_or(TransportError::Backend(
            "uORB: topic not registered — call nros_rmw_uorb::register::<T>(...) first",
        ))?;
        guard.handle().try_recv(buf)
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}
