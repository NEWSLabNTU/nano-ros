//! Subscription handle for receiving typed messages

use core::ffi::c_void;
use core::marker::PhantomData;

use nano_ros_core::{CdrReader, RosMessage};

use zenoh_pico_shim_sys::zenoh_shim_undeclare_subscriber;

/// Subscription for receiving typed messages from a topic
///
/// Created via [`Node::create_subscription`](crate::Node::create_subscription).
/// Automatically undeclared when dropped.
pub struct Subscription<M: RosMessage> {
    handle: i32,
    _marker: PhantomData<M>,
}

impl<M: RosMessage> Subscription<M> {
    /// Create a new subscription with the given handle
    ///
    /// # Safety
    ///
    /// The handle must be a valid subscriber handle from zenoh_shim_declare_subscriber.
    pub(crate) unsafe fn from_handle(handle: i32) -> Self {
        Self {
            handle,
            _marker: PhantomData,
        }
    }
}

impl<M: RosMessage> Drop for Subscription<M> {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_undeclare_subscriber(self.handle);
        }
    }
}

/// Generic trampoline: deserializes CDR and calls user's typed `fn(&M)`
pub(crate) extern "C" fn subscription_trampoline<M: RosMessage>(
    data: *const u8,
    len: usize,
    ctx: *mut c_void,
) {
    let callback: fn(&M) = unsafe { core::mem::transmute(ctx) };
    let bytes = unsafe { core::slice::from_raw_parts(data, len) };
    if let Ok(mut reader) = CdrReader::new_with_header(bytes)
        && let Ok(msg) = M::deserialize(&mut reader)
    {
        callback(&msg);
    }
}
