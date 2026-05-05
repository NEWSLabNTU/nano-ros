//! DDS session — implements `nros_rmw::Session`.

use nros_rmw::{QosSettings, ServiceInfo, Session, TopicInfo, TransportError};

use crate::{
    publisher::DdsPublisher,
    service::{DdsServiceClient, DdsServiceServer},
    subscriber::DdsSubscriber,
};

// Phase 71.4: when an `alloc`-only (no-std) platform is active we own
// an `NrosPlatformRuntime` that `drive_io()` drains on every spin.
// On `std + platform-posix` the stock dust-dds UDP transport spawns
// its own OS threads and does not need external driving, so we skip
// the runtime field entirely.
#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
use crate::runtime::NrosPlatformRuntime;
#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
use crate::sync::Arc;

// ---------------------------------------------------------------------------
// No-listener ZSTs — dust-dds's async create_* methods take
// `Option<impl XListener + Send + 'static>`, so `None::<()>` doesn't
// compile (the unit type doesn't impl the listener traits). One ZST per
// listener trait satisfies the bound at the call site without wiring
// any callbacks.
// ---------------------------------------------------------------------------

#[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
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
    impl<Foo: 'static> dust_dds::dds_async::data_writer_listener::DataWriterListener<Foo>
        for NoDataWriterListener<Foo>
    {
    }

    pub type NoDataWriterListenerRaw = NoDataWriterListener<RawCdrPayload>;
    // No reader-listener variant: every reader callsite passes a real
    // `DataAvailableListener` (the waker bridge) rather than `None`, so
    // `NoDataReaderListener` was structurally unreachable.
}

/// DDS session backed by a dust-dds `DomainParticipant`.
pub struct DdsSession {
    #[cfg(feature = "std")]
    participant: dust_dds::domain::domain_participant::DomainParticipant,
    /// Async participant — used on the no_std path. Methods are
    /// driven through `runtime.block_on(...)`.
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
    participant_async: dust_dds::dds_async::domain_participant::DomainParticipantAsync,
    /// Cooperative runtime driven by `drive_io()`; only present on the
    /// no_std path where dust-dds has no background threads.
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
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
    #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
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

// Phase 71.28 — Service request/reply QoS.
//
// dust-dds DataReader/DataWriter default to `BestEffort + KeepLast(1)`,
// which is fine for high-rate pubsub (lose a sample, the next one
// reaches you anyway) but breaks request/reply: a single dropped
// request packet means the client times out and the server never
// sees the call. ROS 2 service convention is `Reliable +
// KeepLast(N)` on both sides; mirror that here.
// `service_reader_qos` / `service_writer_qos` are only invoked from
// the create_service_{server,client} paths, both of which are gated
// on `cfg(any(feature = "std", feature = "nostd-runtime"))`. Gate
// the helpers on the same cfg so the bare-no-std fallback (which
// has no callers) doesn't emit "function never used" warnings.
// ============================================================================
// Phase 108.B — QoS mapping (nros QosSettings → dust-dds DataWriterQos /
// DataReaderQos). Called from `create_publisher` / `create_subscriber`.
// ============================================================================

#[cfg(any(feature = "std", feature = "nostd-runtime"))]
fn ms_to_duration_kind(ms: u32) -> dust_dds::infrastructure::time::DurationKind {
    use dust_dds::infrastructure::time::{Duration, DurationKind};
    if ms == 0 {
        DurationKind::Infinite
    } else {
        let sec = (ms / 1000) as i32;
        let nanosec = (ms % 1000) * 1_000_000;
        DurationKind::Finite(Duration::new(sec, nanosec))
    }
}

#[cfg(any(feature = "std", feature = "nostd-runtime"))]
fn map_writer_qos(qos: &nros_rmw::QosSettings) -> dust_dds::infrastructure::qos::DataWriterQos {
    use dust_dds::infrastructure::qos_policy::{
        DurabilityQosPolicyKind, HistoryQosPolicyKind, LivelinessQosPolicyKind,
        ReliabilityQosPolicyKind,
    };
    use nros_rmw::{
        QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
    };

    let mut q = dust_dds::infrastructure::qos::DataWriterQos::default();

    q.reliability.kind = match qos.reliability {
        QosReliabilityPolicy::BestEffort => ReliabilityQosPolicyKind::BestEffort,
        QosReliabilityPolicy::Reliable => ReliabilityQosPolicyKind::Reliable,
    };
    q.durability.kind = match qos.durability {
        QosDurabilityPolicy::Volatile => DurabilityQosPolicyKind::Volatile,
        QosDurabilityPolicy::TransientLocal => DurabilityQosPolicyKind::TransientLocal,
    };
    q.history.kind = match qos.history {
        QosHistoryPolicy::KeepLast => HistoryQosPolicyKind::KeepLast(qos.depth),
        QosHistoryPolicy::KeepAll => HistoryQosPolicyKind::KeepAll,
    };
    q.deadline.period = ms_to_duration_kind(qos.deadline_ms);
    q.lifespan.duration = ms_to_duration_kind(qos.lifespan_ms);
    q.liveliness.kind = match qos.liveliness_kind {
        // None → Automatic + infinite lease (no wire activity); avoids
        // adding a "no liveliness" mode that DDS doesn't have.
        QosLivelinessPolicy::None | QosLivelinessPolicy::Automatic => {
            LivelinessQosPolicyKind::Automatic
        }
        QosLivelinessPolicy::ManualByTopic => LivelinessQosPolicyKind::ManualByTopic,
        // ROS "by node" maps to DDS "by participant" — every nano-ros
        // process has one DDS participant.
        QosLivelinessPolicy::ManualByNode => LivelinessQosPolicyKind::ManualByParticipant,
    };
    q.liveliness.lease_duration = ms_to_duration_kind(qos.liveliness_lease_ms);
    q
}

#[cfg(any(feature = "std", feature = "nostd-runtime"))]
fn map_reader_qos(qos: &nros_rmw::QosSettings) -> dust_dds::infrastructure::qos::DataReaderQos {
    use dust_dds::infrastructure::qos_policy::{
        DurabilityQosPolicyKind, HistoryQosPolicyKind, LivelinessQosPolicyKind,
        ReliabilityQosPolicyKind,
    };
    use nros_rmw::{
        QosDurabilityPolicy, QosHistoryPolicy, QosLivelinessPolicy, QosReliabilityPolicy,
    };

    let mut q = dust_dds::infrastructure::qos::DataReaderQos::default();

    q.reliability.kind = match qos.reliability {
        QosReliabilityPolicy::BestEffort => ReliabilityQosPolicyKind::BestEffort,
        QosReliabilityPolicy::Reliable => ReliabilityQosPolicyKind::Reliable,
    };
    q.durability.kind = match qos.durability {
        QosDurabilityPolicy::Volatile => DurabilityQosPolicyKind::Volatile,
        QosDurabilityPolicy::TransientLocal => DurabilityQosPolicyKind::TransientLocal,
    };
    q.history.kind = match qos.history {
        QosHistoryPolicy::KeepLast => HistoryQosPolicyKind::KeepLast(qos.depth),
        QosHistoryPolicy::KeepAll => HistoryQosPolicyKind::KeepAll,
    };
    q.deadline.period = ms_to_duration_kind(qos.deadline_ms);
    q.liveliness.kind = match qos.liveliness_kind {
        QosLivelinessPolicy::None | QosLivelinessPolicy::Automatic => {
            LivelinessQosPolicyKind::Automatic
        }
        QosLivelinessPolicy::ManualByTopic => LivelinessQosPolicyKind::ManualByTopic,
        QosLivelinessPolicy::ManualByNode => LivelinessQosPolicyKind::ManualByParticipant,
    };
    q.liveliness.lease_duration = ms_to_duration_kind(qos.liveliness_lease_ms);
    // Note: DataReaderQos has no `lifespan` field — readers honour the
    // writer's lifespan via per-sample expiry timestamps.
    q
}

#[cfg(any(feature = "std", feature = "nostd-runtime"))]
fn service_reader_qos() -> dust_dds::infrastructure::qos::DataReaderQos {
    use dust_dds::infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        time::{Duration, DurationKind},
    };
    dust_dds::infrastructure::qos::DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: DurationKind::Finite(Duration::new(0, 100_000_000)),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
        },
        ..Default::default()
    }
}

#[cfg(any(feature = "std", feature = "nostd-runtime"))]
fn service_writer_qos() -> dust_dds::infrastructure::qos::DataWriterQos {
    use dust_dds::infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        time::{Duration, DurationKind},
    };
    dust_dds::infrastructure::qos::DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: DurationKind::Finite(Duration::new(0, 100_000_000)),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
        },
        ..Default::default()
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
        qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::{
                raw_type::RawCdrPayload,
                sync::Arc,
                waker_cell::{PublisherEventListener, PublisherShared},
            };
            use dust_dds::infrastructure::{
                qos::QosKind,
                status::{NO_STATUS, StatusKind},
                type_support::TypeSupport,
            };

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

            let shared = Arc::new(PublisherShared::default());
            let writer = publisher
                .create_datawriter::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Specific(map_writer_qos(&qos)),
                    Some(PublisherEventListener::new(shared.clone())),
                    &[
                        StatusKind::LivelinessLost,
                        StatusKind::OfferedDeadlineMissed,
                    ],
                )
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            Ok(DdsPublisher::new(writer, shared))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::{
                raw_type::RawCdrPayload,
                sync::Arc,
                waker_cell::{PublisherEventListener, PublisherShared},
            };
            use dust_dds::infrastructure::{
                qos::QosKind,
                status::{NO_STATUS, StatusKind},
                type_support::TypeSupport,
            };
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

            let shared = Arc::new(PublisherShared::default());
            let writer = self
                .runtime
                .block_on(publisher.create_datawriter::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Specific(map_writer_qos(&qos)),
                    Some(PublisherEventListener::new(shared.clone())),
                    &[
                        StatusKind::LivelinessLost,
                        StatusKind::OfferedDeadlineMissed,
                    ],
                ))
                .map_err(|_| TransportError::PublisherCreationFailed)?;

            return Ok(DdsPublisher::new_async(
                writer,
                self.runtime.clone(),
                shared,
            ));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = (topic, qos);
            Err(TransportError::PublisherCreationFailed)
        }
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        #[cfg(feature = "std")]
        {
            use crate::{
                raw_type::RawCdrPayload,
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::{
                qos::QosKind,
                status::{NO_STATUS, StatusKind},
                type_support::TypeSupport,
            };

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

            let shared = Arc::new(SubscriberShared::default());
            let listener = DataAvailableListener::new(shared.clone());
            // Phase 108.A.dds — register listener for DataAvailable +
            // every Tier-1 sub-side status kind. Cheap (~0 cost when
            // no event callback registered; check is one Mutex peek).
            let reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Specific(map_reader_qos(&qos)),
                    Some(listener),
                    &[
                        StatusKind::DataAvailable,
                        StatusKind::LivelinessChanged,
                        StatusKind::RequestedDeadlineMissed,
                        StatusKind::SampleLost,
                    ],
                )
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            Ok(DdsSubscriber::new(reader, shared))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::{
                raw_type::RawCdrPayload,
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::{
                qos::QosKind,
                status::{NO_STATUS, StatusKind},
                type_support::TypeSupport,
            };
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

            let shared = Arc::new(SubscriberShared::default());
            let listener = DataAvailableListener::new(shared.clone());
            let reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &dds_topic,
                    QosKind::Specific(map_reader_qos(&qos)),
                    Some(listener),
                    &[
                        StatusKind::DataAvailable,
                        StatusKind::LivelinessChanged,
                        StatusKind::RequestedDeadlineMissed,
                        StatusKind::SampleLost,
                    ],
                ))
                .map_err(|_| TransportError::SubscriberCreationFailed)?;

            return Ok(DdsSubscriber::new_async(
                reader,
                self.runtime.clone(),
                shared,
            ));
        }

        #[cfg(not(any(feature = "std", feature = "nostd-runtime")))]
        {
            let _ = (topic, qos);
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
            use dust_dds::infrastructure::{
                qos::QosKind, status::NO_STATUS, type_support::TypeSupport,
            };

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

            use crate::{
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::status::StatusKind;

            let subscriber = self
                .participant
                .create_subscriber(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let req_waker = Arc::new(SubscriberShared::default());
            let req_listener = DataAvailableListener::new(req_waker.clone());
            let request_reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Specific(service_reader_qos()),
                    Some(req_listener),
                    &[StatusKind::DataAvailable],
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            let publisher = self
                .participant
                .create_publisher(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let reply_writer = publisher
                .create_datawriter::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Specific(service_writer_qos()),
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            Ok(DdsServiceServer::new(
                DdsSubscriber::new(request_reader, req_waker),
                DdsPublisher::new(
                    reply_writer,
                    Arc::new(crate::waker_cell::PublisherShared::default()),
                ),
            ))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::{
                qos::QosKind, status::NO_STATUS, type_support::TypeSupport,
            };
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

            use crate::{
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::status::StatusKind;

            let subscriber = self
                .runtime
                .block_on(self.participant_async.create_subscriber(
                    QosKind::Default,
                    None::<NoSubscriberListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;
            let req_waker = Arc::new(SubscriberShared::default());
            let req_listener = DataAvailableListener::new(req_waker.clone());
            let request_reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &req_topic,
                    QosKind::Specific(service_reader_qos()),
                    Some(req_listener),
                    &[StatusKind::DataAvailable],
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
                    QosKind::Specific(service_writer_qos()),
                    None::<NoDataWriterListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceServerCreationFailed)?;

            return Ok(DdsServiceServer::new(
                DdsSubscriber::new_async(request_reader, self.runtime.clone(), req_waker),
                DdsPublisher::new_async(
                    reply_writer,
                    self.runtime.clone(),
                    Arc::new(crate::waker_cell::PublisherShared::default()),
                ),
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
            use dust_dds::infrastructure::{
                qos::QosKind, status::NO_STATUS, type_support::TypeSupport,
            };

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
                    QosKind::Specific(service_writer_qos()),
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            use crate::{
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::status::StatusKind;

            let subscriber = self
                .participant
                .create_subscriber(QosKind::Default, None::<()>, NO_STATUS)
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let reply_waker = Arc::new(SubscriberShared::default());
            let reply_listener = DataAvailableListener::new(reply_waker.clone());
            let reply_reader = subscriber
                .create_datareader::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Specific(service_reader_qos()),
                    Some(reply_listener),
                    &[StatusKind::DataAvailable],
                )
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            Ok(DdsServiceClient::new(
                DdsPublisher::new(
                    request_writer,
                    Arc::new(crate::waker_cell::PublisherShared::default()),
                ),
                DdsSubscriber::new(reply_reader, reply_waker),
            ))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::raw_type::RawCdrPayload;
            use dust_dds::infrastructure::{
                qos::QosKind, status::NO_STATUS, type_support::TypeSupport,
            };
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
                    QosKind::Specific(service_writer_qos()),
                    None::<NoDataWriterListenerRaw>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            use crate::{
                sync::Arc,
                waker_cell::{DataAvailableListener, SubscriberShared},
            };
            use dust_dds::infrastructure::status::StatusKind;

            let subscriber = self
                .runtime
                .block_on(self.participant_async.create_subscriber(
                    QosKind::Default,
                    None::<NoSubscriberListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;
            let reply_waker = Arc::new(SubscriberShared::default());
            let reply_listener = DataAvailableListener::new(reply_waker.clone());
            let reply_reader = self
                .runtime
                .block_on(subscriber.create_datareader::<RawCdrPayload>(
                    &reply_topic,
                    QosKind::Specific(service_reader_qos()),
                    Some(reply_listener),
                    &[StatusKind::DataAvailable],
                ))
                .map_err(|_| TransportError::ServiceClientCreationFailed)?;

            return Ok(DdsServiceClient::new(
                DdsPublisher::new_async(
                    request_writer,
                    self.runtime.clone(),
                    Arc::new(crate::waker_cell::PublisherShared::default()),
                ),
                DdsSubscriber::new_async(reply_reader, self.runtime.clone(), reply_waker),
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
        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            self.runtime.drive();
        }
        Ok(())
    }

    fn supported_qos_policies(&self) -> nros_rmw::QosPolicyMask {
        // Phase 108.B — dust-dds maps every DDS QoS policy nano-ros
        // exposes (durability TL, deadline, lifespan, all liveliness
        // kinds). The only nros-side policy not honoured is
        // `AVOID_ROS_NAMESPACE_CONVENTIONS`, a topic-name-encoding
        // flag handled at the nano-ros layer (not yet wired anywhere).
        use nros_rmw::QosPolicyMask;
        QosPolicyMask::CORE
            | QosPolicyMask::DURABILITY_TRANSIENT_LOCAL
            | QosPolicyMask::DEADLINE
            | QosPolicyMask::LIFESPAN
            | QosPolicyMask::LIVELINESS_AUTOMATIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_TOPIC
            | QosPolicyMask::LIVELINESS_MANUAL_BY_NODE
            | QosPolicyMask::LIVELINESS_LEASE
    }
}
