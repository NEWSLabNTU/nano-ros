//! Concrete session type aliases resolved at compile time.
//!
//! Exactly one RMW backend feature must be enabled. The aliases below
//! map the generic `Session` associated types to the concrete handles
//! provided by the active backend, eliminating the need for generic
//! type parameters on `Executor`, `Node`, and entity types.

use nros_rmw::Session;

#[cfg(feature = "rmw-zenoh")]
pub(crate) type ConcreteSession = nros_rmw_zenoh::ZenohSession;
#[cfg(feature = "rmw-xrce")]
pub(crate) type ConcreteSession = nros_rmw_xrce::XrceSession;
#[cfg(feature = "rmw-cffi")]
pub(crate) type ConcreteSession = nros_rmw_cffi::CffiSession;
#[cfg(all(
    test,
    not(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-cffi"))
))]
pub(crate) type ConcreteSession = crate::mock::MockSession;

/// Concrete publisher handle for the active RMW backend.
pub type RmwPublisher = <ConcreteSession as Session>::PublisherHandle;
/// Concrete subscriber handle for the active RMW backend.
pub type RmwSubscriber = <ConcreteSession as Session>::SubscriberHandle;
/// Concrete service server handle for the active RMW backend.
pub type RmwServiceServer = <ConcreteSession as Session>::ServiceServerHandle;
/// Concrete service client handle for the active RMW backend.
pub type RmwServiceClient = <ConcreteSession as Session>::ServiceClientHandle;
