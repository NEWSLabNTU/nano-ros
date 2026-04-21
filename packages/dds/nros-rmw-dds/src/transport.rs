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
#[derive(Default)]
pub struct DdsRmw;

impl Rmw for DdsRmw {
    type Session = DdsSession;
    type Error = TransportError;

    fn open(self, config: &RmwConfig) -> Result<Self::Session, Self::Error> {
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

        #[cfg(not(feature = "std"))]
        {
            let _ = config;
            Err(TransportError::ConnectionFailed)
        }
    }
}
