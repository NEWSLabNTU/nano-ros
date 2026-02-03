//! Publisher handle for sending messages

use crate::error::{Error, Result};

// Use FFI from zenoh-pico-shim-sys
use zenoh_pico_shim_sys::{zenoh_shim_publish, zenoh_shim_undeclare_publisher};

/// Publisher for sending messages to a topic
///
/// Created via `Node::create_publisher()`.
/// Automatically undeclared when dropped.
pub struct Publisher {
    handle: i32,
}

impl Publisher {
    /// Create a new publisher with the given handle
    ///
    /// # Safety
    ///
    /// The handle must be a valid publisher handle from zenoh_shim_declare_publisher.
    pub(crate) unsafe fn from_handle(handle: i32) -> Self {
        Self { handle }
    }

    /// Publish data to the topic
    ///
    /// # Errors
    ///
    /// Returns `Error::Publish` if the publish operation fails.
    pub fn publish(&self, data: &[u8]) -> Result<()> {
        let ret = unsafe { zenoh_shim_publish(self.handle, data.as_ptr(), data.len()) };
        if ret < 0 {
            return Err(Error::Publish);
        }
        Ok(())
    }

    /// Get the raw handle (for debugging)
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_undeclare_publisher(self.handle);
        }
    }
}
