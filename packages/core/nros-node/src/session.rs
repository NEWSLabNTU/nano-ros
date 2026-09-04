//! Concrete session type aliases resolved at compile time.
//!
//! Exactly one RMW backend feature must be enabled. The aliases below
//! map the generic `Session` associated types to the concrete handles
//! provided by the active backend, eliminating the need for generic
//! type parameters on `Executor`, `Node`, and entity types.

use nros_rmw::Session;

#[cfg(feature = "rmw-cffi")]
pub(crate) type ConcreteSession = nros_rmw_cffi::CffiSession;
#[cfg(all(test, not(feature = "rmw-cffi")))]
pub(crate) type ConcreteSession = crate::mock::MockSession;

/// RFC-0088 — the serialization format the backend this image links speaks,
/// as its cross-image identity string.
///
/// This is a compile-time constant precisely because the backend is selected at
/// compile time (see the aliases above): where ROS 2 asks
/// `rmw_get_serialization_format()` at run time — because it resolves its
/// typesupport through `dlopen` — nano-ros already knows.
///
/// **Only meaningful in a single-backend image.** A bridge image links two
/// backends and has no single answer; it must ask each session instead.
pub const IMAGE_SERIALIZATION_FORMAT: &str = <ConcreteSession as Session>::SERIALIZATION_FORMAT;

/// Image-local discriminant for [`IMAGE_SERIALIZATION_FORMAT`]. Used by the
/// compile-time assertions at entity-creation call sites and by the bridge's
/// one-byte comparison; never persisted, never compared across images.
pub const IMAGE_SERIALIZATION_FORMAT_ID: nros_serdes::format::SerializationFormatId =
    <ConcreteSession as Session>::SERIALIZATION_FORMAT_ID;

/// Concrete publisher handle for the active RMW backend.
pub type RmwPublisher = <ConcreteSession as Session>::PublisherHandle;
/// Concrete subscriber handle for the active RMW backend.
pub type RmwSubscriber = <ConcreteSession as Session>::SubscriptionHandle;
/// Concrete service server handle for the active RMW backend.
pub type RmwServiceServer = <ConcreteSession as Session>::ServiceHandle;
/// Concrete service client handle for the active RMW backend.
pub type RmwServiceClient = <ConcreteSession as Session>::ClientHandle;
