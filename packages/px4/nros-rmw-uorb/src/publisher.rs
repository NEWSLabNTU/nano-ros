//! [`UorbPublisher`] implements [`nros_rmw::Publisher`] over uORB.
//!
//! Phase 99.L design: byte-shaped only. Holds a
//! [`px4_uorb::RawPublication`] which lazily advertises on first
//! `publish` and delegates straight to `orb_publish`. No registry,
//! no name lookup, no critical_section.
//!
//! Construction goes through
//! [`crate::UorbSession::create_publisher_uorb`] which takes the
//! `&'static orb_metadata` pointer + multi-instance index. Higher
//! layers (`nros-px4::uorb::Publisher<T>`) feed `T::metadata()` in.

use nros_rmw::{Publisher, TransportError};
use px4_sys::orb_metadata;
use px4_uorb::{RawPubError, RawPublication};

/// Byte-shaped publisher handle for one uORB topic.
///
/// Stores a [`RawPublication`] that owns the lazy
/// `orb_advert_t`. `publish_raw` calls `orb_publish` directly via
/// the raw FFI wrapper — single FFI call, no name lookup, no lock.
pub struct UorbPublisher {
    inner: RawPublication,
    instance: i32,
}

impl UorbPublisher {
    /// Construct a publisher bound to `metadata` on the given
    /// multi-instance index. Does not advertise yet — the first
    /// `publish_raw` call lazily advertises.
    pub(crate) fn new(metadata: &'static orb_metadata, instance: u8) -> Self {
        Self {
            inner: RawPublication::new(metadata),
            instance: instance as i32,
        }
    }

    /// Borrow the metadata this publisher was constructed with.
    pub fn metadata(&self) -> &'static orb_metadata {
        self.inner.metadata()
    }

    /// Multi-instance index.
    pub fn instance(&self) -> u8 {
        self.instance as u8
    }
}

impl core::fmt::Debug for UorbPublisher {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UorbPublisher")
            .field("instance", &self.instance)
            .finish()
    }
}

impl Publisher for UorbPublisher {
    type Error = TransportError;

    fn publish_raw(&self, data: &[u8]) -> Result<(), Self::Error> {
        match self.inner.publish(data) {
            Ok(()) => Ok(()),
            Err(RawPubError::SizeMismatch) => Err(TransportError::BufferTooSmall),
            Err(RawPubError::Failed) => Err(TransportError::PublishFailed),
        }
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}
