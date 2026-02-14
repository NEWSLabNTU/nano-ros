//! Publisher handle for sending typed messages

use core::marker::PhantomData;

use nros_core::{CdrWriter, RosMessage};

use crate::error::{Error, Result};

use nano_ros_transport_zenoh_sys::{zenoh_shim_publish, zenoh_shim_undeclare_publisher};

/// Publisher for sending typed messages to a topic
///
/// Created via [`Node::create_publisher`](crate::Node::create_publisher).
/// Automatically undeclared when dropped.
pub struct Publisher<M: RosMessage> {
    handle: i32,
    _marker: PhantomData<M>,
}

impl<M: RosMessage> Publisher<M> {
    /// Create a new publisher with the given handle
    ///
    /// # Safety
    ///
    /// The handle must be a valid publisher handle from zenoh_shim_declare_publisher.
    pub(crate) unsafe fn from_handle(handle: i32) -> Self {
        Self {
            handle,
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
        self.publish_raw(writer.as_slice())
    }

    /// Publish pre-encoded CDR bytes (internal)
    fn publish_raw(&self, data: &[u8]) -> Result<()> {
        let ret = unsafe { zenoh_shim_publish(self.handle, data.as_ptr(), data.len()) };
        if ret < 0 {
            return Err(Error::Publish);
        }
        Ok(())
    }
}

impl<M: RosMessage> Drop for Publisher<M> {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_undeclare_publisher(self.handle);
        }
    }
}
