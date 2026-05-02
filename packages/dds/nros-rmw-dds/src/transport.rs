//! DDS RMW factory — implements `nros_rmw::Rmw`.

#[cfg(feature = "std")]
use dust_dds::domain::domain_participant_factory::DomainParticipantFactory;
#[cfg(feature = "std")]
use dust_dds::infrastructure::qos::QosKind;
#[cfg(feature = "std")]
use dust_dds::infrastructure::status::NO_STATUS;

use nros_rmw::{Rmw, RmwConfig, TransportError};

use crate::session::DdsSession;

/// DDS RMW backend factory.
///
/// Opens a DDS `DomainParticipant` using dust-dds with UDP multicast transport.
/// Discovery is brokerless (SPDP/SEDP) — no router or agent needed.
///
/// Two paths:
/// * **`std + platform-posix`** — uses dust-dds's stock
///   `DomainParticipantFactory` singleton + `RtpsUdpTransportParticipantFactory`
///   (3 OS threads per participant).
/// * **`alloc + !std`** (Phase 71) — constructs a
///   `NrosPlatformRuntime<ConcretePlatform>` + `NrosUdpTransportFactory`
///   and a `DomainParticipantFactoryAsync`, then `block_on`s the
///   participant creation. No background OS threads; all RTPS work
///   happens inside `Executor::spin_once()`.
#[derive(Default)]
pub struct DdsRmw;

impl Rmw for DdsRmw {
    type Session = DdsSession;
    type Error = TransportError;

    fn open(self, config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        #[cfg(feature = "debug-cortex-m-semihosting")]
        cortex_m_semihosting::hprintln!("[nros-rmw-dds] DdsRmw::open ENTER");
        #[cfg(feature = "std")]
        {
            let factory = DomainParticipantFactory::get_instance();
            let participant = factory
                .create_participant(
                    config.domain_id as i32,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|_| TransportError::ConnectionFailed)?;

            Ok(DdsSession::new(participant, config.domain_id))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::{
                runtime::NrosPlatformRuntime, sync::Arc, transport_nros::NrosUdpTransportFactory,
            };
            use dust_dds::{
                dds_async::domain_participant_factory::DomainParticipantFactoryAsync,
                infrastructure::{qos::QosKind, status::NO_STATUS},
            };

            // Two clones of the runtime: one consumed by the dust-dds
            // factory, one kept around for `block_on` + driving the
            // session. Both share the same `Arc<spawner>` internally.
            let runtime: NrosPlatformRuntime<nros_platform::ConcretePlatform> =
                NrosPlatformRuntime::new();
            let runtime_arc = Arc::new(runtime.clone());
            let transport = NrosUdpTransportFactory::new(runtime_arc.clone());

            // RTPS GUID prefix bytes. `host_id` derived from the
            // platform's local IPv4 (set via `NROS_LOCAL_IPV4` build
            // env) so two QEMU / board instances on the same RTPS
            // segment generate distinct GUID prefixes — without this,
            // each peer's SPDP looks like its own and gets dropped by
            // dust-dds's self-discovery filter, which kills SEDP and
            // pubsub. `app_id` stays a 0-placeholder for now.
            let app_id = [0u8; 4];
            let host_id = crate::transport_nros::LOCAL_IPV4;
            let factory =
                DomainParticipantFactoryAsync::new(runtime.clone(), app_id, host_id, transport);

            // The async create_participant takes `Option<impl
            // DomainParticipantListener + Send + 'static>` — concrete
            // type would require another generic parameter, so we
            // turbo-fish a never-instantiated bottom type. The
            // concrete fork ships `dust_dds::dds_async::domain_participant_listener::DomainParticipantListener`
            // as a trait; passing `None::<()>` requires `()` to impl
            // it, which it doesn't. Wrap with a tiny zero-sized
            // do-nothing impl below.
            struct NoListener;
            impl dust_dds::dds_async::domain_participant_listener::DomainParticipantListener for NoListener {}
            #[cfg(feature = "debug-cortex-m-semihosting")]
            cortex_m_semihosting::hprintln!("[nros-rmw-dds] DdsRmw::open: pre block_on");
            let participant = runtime
                .block_on(factory.create_participant(
                    config.domain_id as i32,
                    QosKind::Default,
                    None::<NoListener>,
                    NO_STATUS,
                ))
                .map_err(|_| TransportError::ConnectionFailed)?;
            #[cfg(feature = "debug-cortex-m-semihosting")]
            cortex_m_semihosting::hprintln!(
                "[nros-rmw-dds] DdsRmw::open: post block_on, session ready"
            );

            Ok(DdsSession::new_nostd(
                runtime_arc,
                participant,
                config.domain_id,
            ))
        }

        #[cfg(not(any(feature = "std", feature = "alloc")))]
        {
            let _ = config;
            Err(TransportError::ConnectionFailed)
        }
    }
}
