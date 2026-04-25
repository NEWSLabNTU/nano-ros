//! DDS session — implements `nros_rmw::Session`.

use nros_rmw::{QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use crate::publisher::DdsPublisher;
use crate::service::{DdsServiceClient, DdsServiceServer};
use crate::subscriber::DdsSubscriber;

// Phase 71.4: when an `alloc`-only (no-std) platform is active we own
// an `NrosPlatformRuntime` that `drive_io()` drains on every spin.
// On `std + platform-posix` the stock dust-dds UDP transport spawns
// its own OS threads and does not need external driving, so we skip
// the runtime field entirely.
#[cfg(feature = "nostd-runtime")]
use crate::runtime::NrosPlatformRuntime;
#[cfg(feature = "nostd-runtime")]
use alloc::sync::Arc;

// ---------------------------------------------------------------------------
// No-listener ZSTs — dust-dds's async create_* methods take
// `Option<impl XListener + Send + 'static>`, so `None::<()>` doesn't
// compile (the unit type doesn't impl the listener traits). One ZST per
// listener trait satisfies the bound at the call site without wiring
// any callbacks.
// ---------------------------------------------------------------------------

#[cfg(feature = "nostd-runtime")]
mod no_listener {
    use crate::raw_type::RawCdrPayload;
    use core::marker::PhantomData;

    pub struct NoTopicListener;
    impl dust_dds::dds_async::topic_listener::TopicListener for NoTopicListener {}

    pub struct NoPublisherListener;
    impl dust_dds::dds_async::publisher_listener::PublisherListener for NoPublisherListener {}

    pub struct NoSubscriberListener;
    impl dust_dds::dds_async::subscriber_listener::SubscriberListener for NoSubscriberListener {}

    pub struct NoDataWriterListener<Foo>(PhantomData<fn() -> Foo>);
    impl<Foo: 'static>
        dust_dds::dds_async::data_writer_listener::DataWriterListener<Foo>
        for NoDataWriterListener<Foo>
    {
    }

    pub struct NoDataReaderListener<Foo>(PhantomData<fn() -> Foo>);
    impl<Foo: 'static>
        dust_dds::dds_async::data_reader_listener::DataReaderListener<Foo>
        for NoDataReaderListener<Foo>
    {
    }

    pub type NoDataWriterListenerRaw = NoDataWriterListener<RawCdrPayload>;
    pub type NoDataReaderListenerRaw = NoDataReaderListener<RawCdrPayload>;
}

/// DDS session backed by a dust-dds `DomainParticipant`.
pub struct DdsSession {
    #[cfg(feature = "std")]
    participant: dust_dds::domain::domain_participant::DomainParticipant,
    /// Async participant — used on the no_std path. Methods are
    /// driven through `runtime.block_on(...)`.
    #[cfg(feature = "nostd-runtime")]
    participant_async: dust_dds::dds_async::domain_participant::DomainParticipantAsync,
    /// Cooperative runtime driven by `drive_io()`; only present on the
    /// no_std path where dust-dds has no background threads.
    #[cfg(feature = "nostd-runtime")]
    runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
    _domain_id: u32,
}

impl DdsSession {
    #[cfg(feature = "std")]
    pub(crate) fn new(
        participant: dust_dds::domain::domain_participant::DomainParticipant,
        domain_id: u32,
    ) -> Self {
        Self {
            participant,
            _domain_id: domain_id,
        }
    }

    /// Constructor used by the no_std path (Phase 71.2 transport).
    #[cfg(feature = "nostd-runtime")]
    pub(crate) fn new_nostd(
        runtime: Arc<NrosPlatformRuntime<nros_platform::ConcretePlatform>>,
        participant_async: dust_dds::dds_async::domain_participant::DomainParticipantAsync,
        domain_id: u32,
    ) -> Self {
        Self {
            participant_async,
            runtime,
            _domain_id: domain_id,
        }
    }
}

impl Session for DdsSession {
    type Error = TransportError;
    type PublisherHandle = DdsPublisher;
    type SubscriberHandle = DdsSubscriber;
    type ServiceServerHandle = DdsServiceServer;
    type ServiceClientHandle = DdsServiceClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;

            let dds_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    topic.name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            let publisher = self
                .participant
                .create_publisher(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            let writer = publisher
                .create_datawriter::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            Ok(DdsPublisher::new(writer))
        }

        #[cfg(feature = "nostd-runtime")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;
            use no_listener::*;

            let dds_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    topic.name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            let publisher = self
                .runtime
                .block_on(self.participant_async.create_publisher(
                    QosKind::Default,
                    None::<NoPublisherListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            let writer = self
                .runtime
                .block_on(publisher.create_datawriter::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Default,
                    None::<NoDataWriterListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            return Ok(DdsPublisher::new_async(writer, self.runtime.clone()));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = (topic, _qos);
            Err(TransportError::PublisherCreationFailed)
        }
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;

            let dds_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    topic.name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            let subscriber = self
                .participant
                .create_subscriber(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            let reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            Ok(DdsSubscriber::new(reader))
        }

        #[cfg(feature = "nostd-runtime")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;
            use no_listener::*;

            let dds_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    topic.name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            let subscriber = self
                .runtime
                .block_on(self.participant_async.create_subscriber(
                    QosKind::Default,
                    None::<NoSubscriberListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            let reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Default,
                    None::<NoDataReaderListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            return Ok(DdsSubscriber::new_async(reader, self.runtime.clone()));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = (topic, _qos);
            Err(TransportError::SubscriberCreationFailed)
        }
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;

            let req_topic_name =
                alloc::format!("rq{}Request", service.name.trim_start_matches('/'));
            let req_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    &req_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let reply_topic_name =
                alloc::format!("rr{}Reply", service.name.trim_start_matches('/'));
            let reply_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    &reply_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let subscriber = self
                .participant
                .create_subscriber(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let request_reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let publisher = self
                .participant
                .create_publisher(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let reply_writer = publisher
                .create_datawriter::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            Ok(DdsServiceServer::new(
                DdsSubscriber::new(request_reader),
                DdsPublisher::new(reply_writer),
            ))
        }

        #[cfg(feature = "nostd-runtime")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;
            use no_listener::*;

            let req_topic_name =
                alloc::format!("rq{}Request", service.name.trim_start_matches('/'));
            let req_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    &req_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let reply_topic_name =
                alloc::format!("rr{}Reply", service.name.trim_start_matches('/'));
            let reply_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    &reply_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let subscriber = self
                .runtime
                .block_on(self.participant_async.create_subscriber(
                    QosKind::Default,
                    None::<NoSubscriberListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let request_reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Default,
                    None::<NoDataReaderListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let publisher = self
                .runtime
                .block_on(self.participant_async.create_publisher(
                    QosKind::Default,
                    None::<NoPublisherListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let reply_writer = self
                .runtime
                .block_on(publisher.create_datawriter::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Default,
                    None::<NoDataWriterListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            return Ok(DdsServiceServer::new(
                DdsSubscriber::new_async(request_reader, self.runtime.clone()),
                DdsPublisher::new_async(reply_writer, self.runtime.clone()),
            ));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = service;
            Err(TransportError::ServiceServerCreationFailed)
        }
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;

            let req_topic_name =
                alloc::format!("rq{}Request", service.name.trim_start_matches('/'));
            let req_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    &req_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let reply_topic_name =
                alloc::format!("rr{}Reply", service.name.trim_start_matches('/'));
            let reply_topic = self
                .participant
                .create_topic::<RawCdrPayload>(
                    &reply_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let publisher = self
                .participant
                .create_publisher(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let request_writer = publisher
                .create_datawriter::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let subscriber = self
                .participant
                .create_subscriber(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let reply_reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            Ok(DdsServiceClient::new(
                DdsPublisher::new(request_writer),
                DdsSubscriber::new(reply_reader),
            ))
        }

        #[cfg(feature = "nostd-runtime")]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::qos::QosKind;
            use dust_dds::infrastructure::status::NO_STATUS;
            use dust_dds::infrastructure::type_support::TypeSupport;
            use no_listener::*;

            let req_topic_name =
                alloc::format!("rq{}Request", service.name.trim_start_matches('/'));
            let req_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    &req_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let reply_topic_name =
                alloc::format!("rr{}Reply", service.name.trim_start_matches('/'));
            let reply_topic = self
                .runtime
                .block_on(self.participant_async.create_topic::<RawCdrPayload>(
                    &reply_topic_name,
                    RawCdrPayload::get_type_name(),
                    QosKind::Default,
                    None::<NoTopicListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let publisher = self
                .runtime
                .block_on(self.participant_async.create_publisher(
                    QosKind::Default,
                    None::<NoPublisherListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let request_writer = self
                .runtime
                .block_on(publisher.create_datawriter::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Default,
                    None::<NoDataWriterListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            let subscriber = self
                .runtime
                .block_on(self.participant_async.create_subscriber(
                    QosKind::Default,
                    None::<NoSubscriberListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let reply_reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Default,
                    None::<NoDataReaderListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            return Ok(DdsServiceClient::new(
                DdsPublisher::new_async(request_writer, self.runtime.clone()),
                DdsSubscriber::new_async(reply_reader, self.runtime.clone()),
            ));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = service;
            Err(TransportError::ServiceClientCreationFailed)
        }
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        // Phase 71.4: on the no_std path, drive the cooperative runtime
        // once per spin so background RTPS tasks (receive loops,
        // heartbeat timers) make progress. On `std + platform-posix`
        // the stock dust-dds transport uses its own OS threads and
        // `drive_io` stays a pure no-op.
        #[cfg(feature = "nostd-runtime")]
        {
            self.runtime.drive();
        }
        Ok(())
    }
}
