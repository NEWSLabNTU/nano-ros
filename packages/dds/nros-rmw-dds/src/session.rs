//! DDS session — implements `nros_rmw::Session`.

use nros_rmw::{QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use crate::publisher::DdsPublisher;
use crate::service::{DdsServiceClient, DdsServiceServer};
use crate::subscriber::DdsSubscriber;

/// DDS session backed by a dust-dds `DomainParticipant`.
pub struct DdsSession {
    #[cfg(feature = "std")]
    participant: dust_dds::domain::domain_participant::DomainParticipant,
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

        #[cfg(not(feature = "std"))]
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

        #[cfg(not(feature = "std"))]
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

        #[cfg(not(feature = "std"))]
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

        #[cfg(not(feature = "std"))]
        {
            let _ = service;
            Err(TransportError::ServiceClientCreationFailed)
        }
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
}
