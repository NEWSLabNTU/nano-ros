//! [`UorbSession`] implements [`nros_rmw::Session`] over uORB.
//!
//! uORB has no global session object — each [`px4_uorb::Publication`] /
//! [`px4_uorb::Subscription`] owns its handle. `UorbSession` is therefore
//! a near-zero struct that just dispenses handles and returns
//! [`TransportError::InvalidConfig`] for the things uORB doesn't model
//! (drive_io, close — both no-ops since the WorkQueue drives I/O).

use nros_rmw::{Rmw, RmwConfig, ServiceInfo, Session, SessionMode, TopicInfo, TransportError};

use crate::publisher::UorbPublisher;
use crate::service::{UorbServiceClient, UorbServiceServer};
use crate::subscriber::UorbSubscriber;
use crate::topics::lookup_topic;

/// uORB-backed RMW. Construct via `UorbRmw::default()` then call
/// [`Rmw::open`] to obtain a [`UorbSession`].
#[derive(Debug, Default, Clone, Copy)]
pub struct UorbRmw;

/// Per-process uORB RMW session. Currently a unit type — uORB carries no
/// global state we need to track here. May acquire fields once we add
/// service/action support (Phase 90.4).
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

        // Validate session mode — uORB is in-process only, both modes collapse
        // to "local". Reject anything that hints at network use.
        let _ = SessionMode::Client; // suppress unused warning until we use it
        Ok(Self {
            _node_name: node_name,
            _namespace: namespace,
        })
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
    type ServiceServerHandle = UorbServiceServer;
    type ServiceClientHandle = UorbServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo<'_>,
        _qos: nros_rmw::QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        let entry = lookup_topic(topic.name).ok_or(TransportError::InvalidConfig)?;
        UorbPublisher::new(entry, topic.name)
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo<'_>,
        _qos: nros_rmw::QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        let entry = lookup_topic(topic.name).ok_or(TransportError::InvalidConfig)?;
        UorbSubscriber::new(entry, topic.name)
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo<'_>,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        // Phase 90.4b: paired-topic protocol — caller must have already
        // registered <name>/_request and <name>/_reply via
        // nros_rmw_uorb::register::<T>(...).
        UorbServiceServer::new(service.name)
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo<'_>,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        UorbServiceClient::new(service.name)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        // Publisher / Subscriber unadvertise on Drop.
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        // uORB I/O is driven by the PX4 WorkQueue, not the executor.
        // Subscription wakes the hosting WorkItem via ScheduleNow().
        Ok(())
    }
}
