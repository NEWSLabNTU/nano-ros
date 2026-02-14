//! Subscription handle for receiving typed messages (pull-based)

use core::marker::PhantomData;

use nros_core::{CdrReader, RosMessage};
use nros_rmw::Subscriber as SubscriberTrait;
use nros_rmw_zenoh::shim::ShimSubscriber;

use crate::error::{Error, Result};

/// Subscription for receiving typed messages from a topic
///
/// Created via [`Node::create_subscription`](crate::Node::create_subscription).
/// Uses pull-based reception — call [`try_recv`](Self::try_recv) in your main loop.
/// Automatically undeclared when dropped.
pub struct Subscription<M: RosMessage> {
    inner: ShimSubscriber,
    _marker: PhantomData<M>,
}

impl<M: RosMessage> Subscription<M> {
    /// Create a new subscription wrapping a ShimSubscriber
    pub(crate) fn new(inner: ShimSubscriber) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Try to receive the next message (pull-based)
    ///
    /// Uses a 1024-byte stack buffer. For larger messages, use
    /// [`try_recv_with_buffer`](Self::try_recv_with_buffer).
    pub fn try_recv(&mut self) -> Result<Option<M>> {
        self.try_recv_with_buffer::<1024>()
    }

    /// Try to receive with a custom buffer size
    pub fn try_recv_with_buffer<const BUF: usize>(&mut self) -> Result<Option<M>> {
        let mut buf = [0u8; BUF];
        match self.inner.try_recv_raw(&mut buf)? {
            Some(len) => {
                let mut reader = CdrReader::new_with_header(&buf[..len])
                    .map_err(|_| Error::Deserialize)?;
                let msg = M::deserialize(&mut reader)
                    .map_err(|_| Error::Deserialize)?;
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Check if data is available without consuming it
    pub fn has_data(&self) -> bool {
        self.inner.has_data()
    }
}
