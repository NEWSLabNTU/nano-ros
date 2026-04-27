//! [`UorbSubscriber`] implements [`nros_rmw::Subscriber`].
//!
//! Phase 90.2 baseline: holds the resolved [`TopicEntry`]; `try_recv_raw`
//! returns `Ok(None)` (no data) until Phase 90.6 wires the typed
//! `px4_uorb::Subscription<T>` codegen path.

use nros_rmw::{Subscriber, TransportError};

use crate::topics::TopicEntry;

/// Subscriber handle for one ROS 2 topic.
///
/// Holds the topic descriptor and the executor-supplied waker (set via
/// [`Subscriber::register_waker`]). The actual `px4_uorb::Subscription<T>`
/// is constructed lazily by the typed-trampoline registry (Phase 90.6).
#[derive(Debug)]
pub struct UorbSubscriber {
    entry: TopicEntry,
}

impl UorbSubscriber {
    pub(crate) fn new(entry: TopicEntry) -> Result<Self, TransportError> {
        Ok(Self { entry })
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

    fn has_data(&self) -> bool {
        false
    }

    fn try_recv_raw(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        // Phase 90.6 wires Subscription<T>::try_recv() via the typed-trampoline
        // registry (same shape as publisher.rs). Until then there is no data.
        Ok(None)
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}
