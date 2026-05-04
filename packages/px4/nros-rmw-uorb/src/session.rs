//! [`UorbSession`] implements [`nros_rmw::Session`] over uORB.
//!
//! Phase 99.L design: byte-shaped only. uORB has no global session
//! object; this is a near-zero struct that just dispenses publisher
//! / subscriber handles.
//!
//! The generic `Session` trait's `create_publisher` /
//! `create_subscriber` cannot work for uORB — the trait only
//! receives `(name, type_name, type_hash)`, none of which yield a
//! `&'static orb_metadata` pointer (uORB needs the full descriptor,
//! not just a name string). Those impls return
//! [`TransportError::Unsupported`] with a message pointing the
//! caller at [`UorbSession::create_publisher_uorb`].
//!
//! Higher layers (`nros-px4::uorb::Publisher<T>`, …) call the
//! `_uorb` byte-shaped methods directly via [`Node::session_mut`]
//! after resolving `T::metadata()`.

use nros_rmw::{Rmw, RmwConfig, ServiceInfo, Session, SessionMode, TopicInfo, TransportError};
use px4_sys::orb_metadata;

use crate::{
    publisher::UorbPublisher,
    service::{UorbServiceClient, UorbServiceServer},
    subscriber::UorbSubscriber,
};

/// uORB-backed RMW. Construct via `UorbRmw::default()` then call
/// [`Rmw::open`] to obtain a [`UorbSession`].
#[derive(Debug, Default, Clone, Copy)]
pub struct UorbRmw;

/// Per-process uORB RMW session. Carries the node-name + namespace
/// (used by the diagnostic surface only — uORB itself is in-process
/// and has no global session state).
#[derive(Debug)]
pub struct UorbSession {
    _node_name: heapless::String<64>,
    _namespace: heapless::String<64>,
}

impl UorbSession {
    fn new(cfg: &RmwConfig<'_>) -> Result<Self, TransportError> {
        let mut node_name = heapless::String::new();
        let _ = node_name.push_str(cfg.node_name);
        let mut namespace = heapless::String::new();
        let _ = namespace.push_str(cfg.namespace);
        let _ = SessionMode::Client; // suppress unused warning
        Ok(Self {
            _node_name: node_name,
            _namespace: namespace,
        })
    }

    /// Byte-shaped uORB publisher creation. Takes the topic's static
    /// `orb_metadata` pointer + multi-instance index directly. The
    /// returned publisher lazily advertises on first `publish_raw`.
    pub fn create_publisher_uorb(
        &mut self,
        metadata: &'static orb_metadata,
        instance: u8,
    ) -> Result<UorbPublisher, TransportError> {
        Ok(UorbPublisher::new(metadata, instance))
    }

    /// Byte-shaped uORB subscriber creation. The returned subscriber
    /// lazy-registers an `orb_register_callback` on first
    /// `try_recv_raw` / `register_waker`.
    pub fn create_subscription_uorb(
        &mut self,
        metadata: &'static orb_metadata,
        instance: u8,
    ) -> Result<UorbSubscriber, TransportError> {
        Ok(UorbSubscriber::new(metadata, instance))
    }
}

impl Rmw for UorbRmw {
    type Session = UorbSession;
    type Error = TransportError;

    fn open(self, config: &RmwConfig<'_>) -> Result<Self::Session, Self::Error> {
        UorbSession::new(config)
    }
}

impl Session for UorbSession {
    type Error = TransportError;
    type PublisherHandle = UorbPublisher;
    type SubscriberHandle = UorbSubscriber;
    // uORB service support is a Phase 99.L follow-up — services need a
    // distinct paired-topic protocol that doesn't fit the byte-shaped
    // metadata-based create path. For now the trait associated types
    // alias to publisher/subscriber so the trait is satisfied; the
    // create_service_* methods below return Unsupported.
    type ServiceServerHandle = UorbServiceServer;
    type ServiceClientHandle = UorbServiceClient;

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo<'_>,
        _qos: nros_rmw::QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        // Generic create_publisher cannot resolve `&'static orb_metadata`
        // from a name + type_name + type_hash. Use
        // UorbSession::create_publisher_uorb (called via
        // Node::session_mut from nros-px4::uorb).
        Err(TransportError::Unsupported)
    }

    fn create_subscriber(
        &mut self,
        _topic: &TopicInfo<'_>,
        _qos: nros_rmw::QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        Err(TransportError::Unsupported)
    }

    fn create_service_server(
        &mut self,
        _service: &ServiceInfo<'_>,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        Err(TransportError::Unsupported)
    }

    fn create_service_client(
        &mut self,
        _service: &ServiceInfo<'_>,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        Err(TransportError::Unsupported)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        // Publisher / Subscriber unadvertise / unregister on Drop.
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        // uORB I/O is driven by the PX4 WorkQueue, not the executor.
        // Subscription wakes the hosting WorkItem via the AtomicWaker
        // attached to each UorbSubscriber.
        Ok(())
    }

    fn supported_qos_policies(&self) -> nros_rmw::QosPolicyMask {
        // Phase 108.B — uORB is intra-process pubsub w/ no wire-level
        // reliability or durability negotiation. Adapted semantics:
        // RELIABLE always (queue-bounded, no drops while consumer is
        // keeping up); VOLATILE always (no late-joiner replay);
        // HISTORY/DEPTH honoured via the per-subscriber ring buffer.
        // No deadline, lifespan, liveliness, or TL durability.
        nros_rmw::QosPolicyMask::CORE
    }
}
