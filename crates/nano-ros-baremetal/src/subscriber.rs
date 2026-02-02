//! Subscriber handle for receiving messages

// FFI declarations for zenoh-pico shim
extern "C" {
    fn zenoh_shim_undeclare_subscriber(handle: i32) -> i32;
}

/// Subscriber for receiving messages from a topic
///
/// Created via `BaremetalNode::create_subscriber()`.
/// Automatically undeclared when dropped.
///
/// Note: Messages are delivered via callback registered during creation.
pub struct Subscriber {
    handle: i32,
}

impl Subscriber {
    /// Create a new subscriber with the given handle
    ///
    /// # Safety
    ///
    /// The handle must be a valid subscriber handle from zenoh_shim_declare_subscriber.
    pub(crate) unsafe fn from_handle(handle: i32) -> Self {
        Self { handle }
    }

    /// Get the raw handle (for debugging)
    pub fn handle(&self) -> i32 {
        self.handle
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_undeclare_subscriber(self.handle);
        }
    }
}
