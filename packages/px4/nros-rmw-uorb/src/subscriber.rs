//! [`UorbSubscriber`] implements [`nros_rmw::Subscriber`] over uORB.
//!
//! Phase 99.L design: byte-shaped only. Holds a
//! [`px4_uorb::RawSubscription`] which lazily registers an
//! `orb_register_callback` on first `try_recv_raw` and writes
//! into the caller's buffer on each subsequent call. No registry,
//! no name lookup, no critical_section.

use nros_rmw::{Subscriber, TransportError};
use px4_sys::orb_metadata;
use px4_uorb::RawSubscription;

/// Byte-shaped subscriber handle for one uORB topic.
///
/// Holds the raw subscription state directly. `try_recv_raw` calls
/// `orb_copy` (via the raw FFI wrapper) through the lazy
/// callback registration — single FFI path, no lookup, no lock.
pub struct UorbSubscriber {
    inner: RawSubscription,
}

impl UorbSubscriber {
    /// Construct a subscriber bound to `metadata` on the given
    /// multi-instance index. Lazy-registers the underlying
    /// callback on first `try_recv_raw` / `register_waker`.
    pub(crate) fn new(metadata: &'static orb_metadata, instance: u8) -> Self {
        Self {
            inner: RawSubscription::with_instance(metadata, instance),
        }
    }

    /// Borrow the metadata this subscriber was constructed with.
    pub fn metadata(&self) -> &'static orb_metadata {
        self.inner.metadata()
    }
}

impl core::fmt::Debug for UorbSubscriber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UorbSubscriber").finish()
    }
}

impl Subscriber for UorbSubscriber {
    type Error = TransportError;

    fn try_recv_raw(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        Ok(self.inner.try_recv(buf))
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }

    fn register_waker(&self, waker: &core::task::Waker) {
        self.inner.register_waker(waker);
    }
}
