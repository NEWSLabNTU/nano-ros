//! Publisher handle for sending typed messages

use core::marker::PhantomData;

use nros_core::{CdrWriter, RosMessage};
use nros_rmw::Publisher as PublisherTrait;
use nros_rmw_zenoh::shim::ShimPublisher;

use crate::error::{Error, Result};

/// Publisher for sending typed messages to a topic
///
/// Created via [`Node::create_publisher`](crate::Node::create_publisher).
/// Automatically undeclared when dropped.
pub struct Publisher<M: RosMessage> {
    inner: ShimPublisher,
    _marker: PhantomData<M>,
}

impl<M: RosMessage> Publisher<M> {
    /// Create a new publisher wrapping a ShimPublisher
    pub(crate) fn new(inner: ShimPublisher) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }

    /// Publish a typed message (CDR-serialized automatically)
    ///
    /// Uses a 256-byte stack buffer. For larger messages, use
    /// [`publish_with_buffer`](Self::publish_with_buffer).
    pub fn publish(&self, msg: &M) -> Result<()> {
        self.publish_with_buffer::<256>(msg)
    }

    /// Publish a typed message with a custom stack buffer size
    pub fn publish_with_buffer<const BUF: usize>(&self, msg: &M) -> Result<()> {
        let mut buf = [0u8; BUF];
        let mut writer =
            CdrWriter::new_with_header(&mut buf).map_err(|_| Error::BufferTooSmall)?;
        msg.serialize(&mut writer)
            .map_err(|_| Error::Serialize)?;
        self.inner.publish_raw(writer.as_slice())?;
        Ok(())
    }
}
