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

        // Phase 115.H follow-up — locator-scheme dispatch.
        //
        // `custom/...` ⇒ runtime-pluggable byte-pipe transport via
        // `NrosCustomTransportParticipantFactory`. Anything else falls
        // through to the existing UDP path.
        //
        // The custom path requires `DomainParticipantFactoryAsync`
        // (stock `DomainParticipantFactory::get_instance()` is a
        // singleton wired to the UDP transport). On the std build that
        // means we have to switch to the no_std-style async runtime
        // even though stdlib threads are available — the stock UDP
        // factory just isn't extensible. Routing custom dispatch
        // through the async path on std is the "POSIX std-path async
        // factory wiring" follow-up; for now we error out so the
        // limitation is visible rather than silently UDP-fall-through.
        let custom_locator = config.locator.starts_with("custom/");

        #[cfg(feature = "std")]
        {
            if custom_locator {
                // Std build with `custom/...` locator — see the
                // comment above. Surfacing the limitation as
                // `ConnectionFailed` keeps the error-channel narrow
                // (no new variant) while making it impossible to
                // accidentally fall through to UDP.
                return Err(TransportError::ConnectionFailed);
            }
            let factory = DomainParticipantFactory::get_instance();
            let participant = factory
                .create_participant(
                    config.domain_id as i32,
                    QosKind::Default,
                    None::<()>,
                    NO_STATUS,
                )
                .map_err(|e| {
                    if std::env::var_os("NROS_RMW_TRACE_OPEN").is_some() {
                        std::eprintln!("[dust-dds] create_participant failed: {:?}", e);
                    }
                    TransportError::ConnectionFailed
                })?;

            Ok(DdsSession::new(participant, config.domain_id))
        }

        #[cfg(all(feature = "nostd-runtime", not(feature = "std")))]
        {
            use crate::{runtime::NrosPlatformRuntime, sync::Arc};
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

            // RTPS GUID prefix bytes. `host_id` derived from the
            // platform's local IPv4 (set via `NROS_LOCAL_IPV4` build
            // env) so two QEMU / board instances on the same RTPS
            // segment generate distinct GUID prefixes — without this,
            // each peer's SPDP looks like its own and gets dropped by
            // dust-dds's self-discovery filter, which kills SEDP and
            // pubsub. `app_id` stays a 0-placeholder for now.
            let app_id = [0u8; 4];
            let host_id = crate::transport_nros::LOCAL_IPV4;

            // The async create_participant takes `Option<impl
            // DomainParticipantListener + Send + 'static>` — concrete
            // type would require another generic parameter, so we
            // turbo-fish a never-instantiated bottom type.
            struct NoListener;
            impl dust_dds::dds_async::domain_participant_listener::DomainParticipantListener for NoListener {}

            // Branch on locator scheme. The two arms differ only in
            // the `T: TransportParticipantFactory` they hand to the
            // async factory; everything downstream is identical.
            let close_ops = if custom_locator {
                use crate::transport_custom::NrosCustomTransportParticipantFactory;
                let custom = NrosCustomTransportParticipantFactory::from_slot(runtime_arc.clone())
                    .ok_or(TransportError::ConnectionFailed)?;
                let ops_for_close = custom.ops();
                let factory =
                    DomainParticipantFactoryAsync::new(runtime.clone(), app_id, host_id, custom);
                let participant = runtime
                    .block_on(factory.create_participant(
                        config.domain_id as i32,
                        QosKind::Default,
                        None::<NoListener>,
                        NO_STATUS,
                    ))
                    .map_err(|_| TransportError::ConnectionFailed)?;
                return Ok(DdsSession::new_nostd_custom(
                    runtime_arc,
                    participant,
                    config.domain_id,
                    ops_for_close,
                ));
            } else {
                use crate::transport_nros::NrosUdpTransportFactory;
                NrosUdpTransportFactory::new(runtime_arc.clone())
            };
            let _ = close_ops; // unit binding for the UDP arm

            #[cfg(feature = "debug-cortex-m-semihosting")]
            cortex_m_semihosting::hprintln!("[nros-rmw-dds] DdsRmw::open: pre block_on");
            let factory =
                DomainParticipantFactoryAsync::new(runtime.clone(), app_id, host_id, close_ops);
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
            let _ = (config, custom_locator);
            Err(TransportError::ConnectionFailed)
        }
    }
}
