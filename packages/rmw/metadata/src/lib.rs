//! phase-308 — the metadata-mode RMW backend.
//!
//! An RMW backend that records what a component DECLARES instead of
//! transporting it. It is how the C/C++ producer works, and it is deliberately
//! a backend rather than a special mode inside `nros-cpp`: backend selection is
//! an existing, supported extension point (RFC-0054's vtable, chosen by name
//! through `$NROS_RMW`), so `nros_cpp_init` needs no metadata-mode branch and
//! the shipping runtime keeps exactly one code path.
//!
//! # What it does and does not see
//!
//! Publishers, subscriptions, services and service clients all reach the
//! session, so this backend sees every one of them with its name and type.
//!
//! **Timers and guard conditions do not.** They are registered directly on the
//! executor (`register_timer_on`, `register_guard_condition`) and never touch
//! the RMW, so no backend can observe them — which matters more here than
//! anywhere else, because a timer is precisely the entity the SystemModel
//! cannot see either. `nros-cpp`'s hooks record those two, and the split is
//! exactly two functions wide.
//!
//! Note the symmetry with issue 0257: launch wiring has no timer entity, and
//! the RMW has no timer call. Same blind spot, two layers.
//!
//! # Layer discipline
//!
//! This crate is an ADAPTER. It contains no JSON, no schema struct and no slot
//! arithmetic — all three live once, in `nros::node_metadata`, reached through
//! `nros::metadata_mode`. If any of them appears here, the phase-308 layer
//! boundary has been crossed and the count this whole mechanism exists to
//! produce has two definitions again.
//!
//! # Reads and receives
//!
//! Every receive path returns "nothing available" and every send succeeds
//! without doing anything. A probe runs a component's declaration path and
//! exits; it never spins, so nothing observes these. They are honest no-ops
//! rather than `unimplemented!()` so a component whose `configure` happens to
//! publish once during setup does not abort the probe.

use nros::node_metadata::EntityKind;
use nros_rmw::{
    ClientTrait, Publisher, Rmw, RmwConfig, ServiceInfo, ServiceRequest, ServiceTrait, Session,
    Subscription, TopicInfo, TransportError,
};

/// The backend factory. Registered under the name `metadata`.
#[derive(Debug, Default)]
pub struct MetadataRmw;

/// A session that transports nothing.
#[derive(Debug, Default)]
pub struct MetadataSession;

/// Entity handles carry no state: nothing is ever sent or received through
/// them, and the recording already happened at creation time.
#[derive(Debug, Default)]
pub struct MetadataPublisher;
#[derive(Debug, Default)]
pub struct MetadataSubscription;
#[derive(Debug, Default)]
pub struct MetadataService;
#[derive(Debug, Default)]
pub struct MetadataClient;

impl Rmw for MetadataRmw {
    type Session = MetadataSession;
    type Error = TransportError;

    fn open(self, _config: &RmwConfig<'_>) -> Result<Self::Session, Self::Error> {
        // No node is opened here. The session's config names ONE node, but a
        // component may declare several, and the ABI's `nros_cpp_node_create*`
        // hook is what opens each — see `nros::metadata_mode::begin_node`.
        Ok(MetadataSession)
    }
}

/// Record or fail loudly.
///
/// A refused record means the recorder is full or no node is open, and either
/// way the sidecar would under-count. Under-counting is the failure this
/// mechanism exists to prevent (issue 0257: an executor sized too small dies at
/// boot), so it must never be swallowed — the probe reports it as an entity
/// creation failure, which the driver surfaces.
fn record(
    kind: EntityKind,
    name: &str,
    type_name: &str,
    period_ms: Option<u64>,
) -> Result<(), TransportError> {
    if nros::metadata_mode::record_entity(kind, name, type_name, None, period_ms) {
        Ok(())
    } else {
        Err(TransportError::PublisherCreationFailed)
    }
}

impl Session for MetadataSession {
    type Error = TransportError;
    type PublisherHandle = MetadataPublisher;
    type SubscriptionHandle = MetadataSubscription;
    type ServiceHandle = MetadataService;
    type ClientHandle = MetadataClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo<'_>,
        _qos: nros_rmw::QoSProfile,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        record(EntityKind::Publisher, topic.name, topic.type_name, None)?;
        Ok(MetadataPublisher)
    }

    fn create_subscription(
        &mut self,
        topic: &TopicInfo<'_>,
        _qos: nros_rmw::QoSProfile,
    ) -> Result<Self::SubscriptionHandle, Self::Error> {
        record(EntityKind::Subscription, topic.name, topic.type_name, None)?;
        Ok(MetadataSubscription)
    }

    fn create_service(
        &mut self,
        service: &ServiceInfo<'_>,
        _qos: nros_rmw::QoSProfile,
    ) -> Result<Self::ServiceHandle, Self::Error> {
        record(
            EntityKind::ServiceServer,
            service.name,
            service.type_name,
            None,
        )?;
        Ok(MetadataService)
    }

    fn create_client(
        &mut self,
        service: &ServiceInfo<'_>,
        _qos: nros_rmw::QoSProfile,
    ) -> Result<Self::ClientHandle, Self::Error> {
        record(
            EntityKind::ServiceClient,
            service.name,
            service.type_name,
            None,
        )?;
        Ok(MetadataClient)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Publisher for MetadataPublisher {
    type Error = TransportError;

    fn publish_raw(&self, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }

    fn serialization_error(&self) -> Self::Error {
        TransportError::SerializationError
    }
}

impl Subscription for MetadataSubscription {
    type Error = TransportError;

    fn try_recv_raw(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        Ok(None)
    }

    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

impl ServiceTrait for MetadataService {
    type Error = TransportError;

    fn try_recv_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        Ok(None)
    }

    fn send_response(&mut self, _sequence_number: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ClientTrait for MetadataClient {
    type Error = TransportError;

    /// issue 0778 — the trait now hands the caller the request id it just
    /// issued, so a client can match a reply to its own call. This backend
    /// never delivers anything (it exists so a metadata probe can OPEN a
    /// session without a transport), so the id is a constant: there is no
    /// second outstanding request for it to be confused with.
    fn send_request_raw(&mut self, _request: &[u8]) -> Result<i64, Self::Error> {
        Ok(0)
    }

    fn try_recv_reply_raw(
        &mut self,
        _reply_buf: &mut [u8],
    ) -> Result<Option<(usize, i64)>, Self::Error> {
        Ok(None)
    }
}

/// Install this backend into the cffi registry under the name `metadata`.
///
/// Registered by NAME, not as the default: a probe binary may well link a real
/// backend too (the component's own dependencies pull one in), and selecting
/// between them is what `$NROS_RMW` is for. Registering as default would make
/// the choice ambiguous and the executor would refuse to open at all.
#[unsafe(no_mangle)]
pub extern "C" fn nros_rmw_metadata_register() -> nros_rmw_cffi::NrosRmwRet {
    unsafe {
        nros_rmw_cffi::RustBackendAdapter::<MetadataRmw>::register_named(c"metadata".as_ptr())
    }
}

// Hosted self-registration: the probe is a host binary, so the `.init_array`
// ctor fires before `main` and the backend is present without the probe naming
// it. Expands to nothing on `target_os = "none"` — which is also the reason
// this crate can never accidentally register itself into firmware.
nros_rmw_cffi::nros_rmw_register_backend! {
    fn() {
        let _ = nros_rmw_metadata_register();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// The recorder is process-global by design (see `nros::metadata_mode`), so
    /// tests touching it cannot run concurrently under cargo's thread-per-test
    /// harness.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// The backend records through the shared recorder and nothing else — the
    /// entities land in `nros::metadata_mode`, not in any local state here.
    #[test]
    fn session_creates_record_entities() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        nros::metadata_mode::reset();
        assert!(nros::metadata_mode::begin_node("cpp_talker", "/", 0));

        let mut session = MetadataRmw.open(&RmwConfig::default()).expect("open");
        let topic = TopicInfo::new("/chatter", "std_msgs/msg/Int32", "");
        session
            .create_publisher(&topic, Default::default())
            .expect("pub");
        session
            .create_subscription(&topic, Default::default())
            .expect("sub");
        let service = ServiceInfo::new("/add", "example_interfaces/srv/AddTwoInts", "");
        session
            .create_service(&service, Default::default())
            .expect("srv");
        session
            .create_client(&service, Default::default())
            .expect("cli");

        assert_eq!(nros::metadata_mode::entity_count(), 4);
        nros::metadata_mode::reset();
    }

    /// Recording into no open node must FAIL the create, not silently drop the
    /// entity — a dropped entity is an under-sized executor at boot.
    #[test]
    fn creating_without_an_open_node_fails_loudly() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        nros::metadata_mode::reset();
        let mut session = MetadataRmw.open(&RmwConfig::default()).expect("open");
        let topic = TopicInfo::new("/chatter", "std_msgs/msg/Int32", "");
        assert!(
            session
                .create_publisher(&topic, Default::default())
                .is_err()
        );
        nros::metadata_mode::reset();
    }
}
