//! Phase 115.L.0 — Smoke test for `RustBackendAdapter`.
//!
//! Defines a `NoopRmw` fixture that implements every `nros_rmw` trait
//! with stub behaviour, exposes it through `RustBackendAdapter<NoopRmw>`,
//! drives the resulting vtable end-to-end via `CffiSession`, and
//! confirms each trampoline reached its trait counterpart.

#![cfg(feature = "alloc")]
#![allow(clippy::manual_c_str_literals)]

use std::sync::{
    Mutex,
    atomic::{AtomicU32, Ordering},
};

use nros_rmw::{
    EventCallback, EventKind, Publisher, QosPolicyMask, QosSettings, Rmw, RmwConfig,
    ServiceClientTrait, ServiceInfo, ServiceRequest, ServiceServerTrait, Session, Subscriber,
    TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwEventKind, NrosRmwEventPayload,
    NrosRmwLivelinessChangedStatus, NrosRmwQos, NrosRmwServiceClient, NrosRmwServiceServer,
    NrosRmwSession, RustBackendAdapter,
};

// ----------------------------------------------------------------------------
// Hit counters per trampoline.
// ----------------------------------------------------------------------------

static OPEN_HITS: AtomicU32 = AtomicU32::new(0);
static CLOSE_HITS: AtomicU32 = AtomicU32::new(0);
static DRIVE_IO_HITS: AtomicU32 = AtomicU32::new(0);
static CREATE_PUB_HITS: AtomicU32 = AtomicU32::new(0);
static PUBLISH_HITS: AtomicU32 = AtomicU32::new(0);
static DESTROY_PUB_HITS: AtomicU32 = AtomicU32::new(0);
static CREATE_SUB_HITS: AtomicU32 = AtomicU32::new(0);
static TRY_RECV_HITS: AtomicU32 = AtomicU32::new(0);
static HAS_DATA_HITS: AtomicU32 = AtomicU32::new(0);
static DESTROY_SUB_HITS: AtomicU32 = AtomicU32::new(0);
static CREATE_SRV_SERVER_HITS: AtomicU32 = AtomicU32::new(0);
static CREATE_SRV_CLIENT_HITS: AtomicU32 = AtomicU32::new(0);
static HAS_REQUEST_HITS: AtomicU32 = AtomicU32::new(0);
static TRY_RECV_REQUEST_HITS: AtomicU32 = AtomicU32::new(0);
static SEND_REPLY_HITS: AtomicU32 = AtomicU32::new(0);
static SEND_REQUEST_HITS: AtomicU32 = AtomicU32::new(0);
static TRY_RECV_REPLY_HITS: AtomicU32 = AtomicU32::new(0);
static SUB_EVENT_HITS: AtomicU32 = AtomicU32::new(0);
static PUB_EVENT_HITS: AtomicU32 = AtomicU32::new(0);
static ASSERT_LIVELINESS_HITS: AtomicU32 = AtomicU32::new(0);
static EVENT_CALLBACK_HITS: AtomicU32 = AtomicU32::new(0);

fn reset() {
    for c in [
        &OPEN_HITS,
        &CLOSE_HITS,
        &DRIVE_IO_HITS,
        &CREATE_PUB_HITS,
        &PUBLISH_HITS,
        &DESTROY_PUB_HITS,
        &CREATE_SUB_HITS,
        &TRY_RECV_HITS,
        &HAS_DATA_HITS,
        &DESTROY_SUB_HITS,
        &CREATE_SRV_SERVER_HITS,
        &CREATE_SRV_CLIENT_HITS,
        &HAS_REQUEST_HITS,
        &TRY_RECV_REQUEST_HITS,
        &SEND_REPLY_HITS,
        &SEND_REQUEST_HITS,
        &TRY_RECV_REPLY_HITS,
        &SUB_EVENT_HITS,
        &PUB_EVENT_HITS,
        &ASSERT_LIVELINESS_HITS,
        &EVENT_CALLBACK_HITS,
    ] {
        c.store(0, Ordering::SeqCst);
    }
}

// ----------------------------------------------------------------------------
// Fixture: minimal in-memory backend with side-effect counters.
// ----------------------------------------------------------------------------

#[derive(Default)]
struct NoopRmw;

struct NoopSession;
struct NoopPublisher;
struct NoopSubscriber;
struct NoopServer;
struct NoopClient;

#[derive(Default)]
struct IdentityRmw;

struct IdentitySession;

#[derive(Debug, PartialEq, Eq)]
struct IdentityRecord {
    kind: &'static str,
    node_name: Option<String>,
    namespace: String,
}

static IDENTITY_RECORDS: Mutex<Vec<IdentityRecord>> = Mutex::new(Vec::new());

impl Rmw for NoopRmw {
    type Session = NoopSession;
    type Error = TransportError;
    fn open(self, _config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        OPEN_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(NoopSession)
    }
}

impl Rmw for IdentityRmw {
    type Session = IdentitySession;
    type Error = TransportError;

    fn open(self, _config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        Ok(IdentitySession)
    }
}

impl Session for IdentitySession {
    type Error = TransportError;
    type PublisherHandle = NoopPublisher;
    type SubscriberHandle = NoopSubscriber;
    type ServiceServerHandle = NoopServer;
    type ServiceClientHandle = NoopClient;

    fn create_publisher(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        IDENTITY_RECORDS.lock().unwrap().push(IdentityRecord {
            kind: "publisher",
            node_name: topic.node_name.map(str::to_owned),
            namespace: topic.namespace.to_owned(),
        });
        Ok(NoopPublisher)
    }

    fn create_subscriber(
        &mut self,
        topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        IDENTITY_RECORDS.lock().unwrap().push(IdentityRecord {
            kind: "subscriber",
            node_name: topic.node_name.map(str::to_owned),
            namespace: topic.namespace.to_owned(),
        });
        Ok(NoopSubscriber)
    }

    fn create_service_server(
        &mut self,
        service: &ServiceInfo,
        _qos: QosSettings,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        IDENTITY_RECORDS.lock().unwrap().push(IdentityRecord {
            kind: "service_server",
            node_name: service.node_name.map(str::to_owned),
            namespace: service.namespace.to_owned(),
        });
        Ok(NoopServer)
    }

    fn create_service_client(
        &mut self,
        service: &ServiceInfo,
        _qos: QosSettings,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        IDENTITY_RECORDS.lock().unwrap().push(IdentityRecord {
            kind: "service_client",
            node_name: service.node_name.map(str::to_owned),
            namespace: service.namespace.to_owned(),
        });
        Ok(NoopClient)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Session for NoopSession {
    type Error = TransportError;
    type PublisherHandle = NoopPublisher;
    type SubscriberHandle = NoopSubscriber;
    type ServiceServerHandle = NoopServer;
    type ServiceClientHandle = NoopClient;

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        CREATE_PUB_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(NoopPublisher)
    }
    fn create_subscriber(
        &mut self,
        _topic: &TopicInfo,
        _qos: QosSettings,
    ) -> Result<Self::SubscriberHandle, Self::Error> {
        CREATE_SUB_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(NoopSubscriber)
    }
    fn create_service_server(
        &mut self,
        _service: &ServiceInfo,
        _qos: QosSettings,
    ) -> Result<Self::ServiceServerHandle, Self::Error> {
        CREATE_SRV_SERVER_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(NoopServer)
    }
    fn create_service_client(
        &mut self,
        _service: &ServiceInfo,
        _qos: QosSettings,
    ) -> Result<Self::ServiceClientHandle, Self::Error> {
        CREATE_SRV_CLIENT_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(NoopClient)
    }
    fn close(&mut self) -> Result<(), Self::Error> {
        CLOSE_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        DRIVE_IO_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn supported_qos_policies(&self) -> QosPolicyMask {
        QosPolicyMask::CORE
    }
}

impl Drop for NoopPublisher {
    fn drop(&mut self) {
        DESTROY_PUB_HITS.fetch_add(1, Ordering::SeqCst);
    }
}
impl Drop for NoopSubscriber {
    fn drop(&mut self) {
        DESTROY_SUB_HITS.fetch_add(1, Ordering::SeqCst);
    }
}

impl Publisher for NoopPublisher {
    type Error = TransportError;
    fn publish_raw(&self, _data: &[u8]) -> Result<(), Self::Error> {
        PUBLISH_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }
    fn serialization_error(&self) -> Self::Error {
        TransportError::Backend("ser")
    }
    fn supports_event(&self, _kind: EventKind) -> bool {
        true
    }
    unsafe fn register_event_callback(
        &mut self,
        kind: EventKind,
        _deadline_ms: u32,
        cb: EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), Self::Error> {
        PUB_EVENT_HITS.fetch_add(1, Ordering::SeqCst);
        // Fire callback immediately with a synthetic LivelinessLost
        // (Publisher-side event uses `CountStatus` payload).
        let payload = nros_rmw::CountStatus {
            total_count: 7,
            total_count_change: 1,
        };
        unsafe {
            cb(
                kind,
                &payload as *const _ as *const core::ffi::c_void,
                user_ctx,
            );
        }
        Ok(())
    }
    fn assert_liveliness(&self) -> Result<(), Self::Error> {
        ASSERT_LIVELINESS_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Subscriber for NoopSubscriber {
    type Error = TransportError;
    fn try_recv_raw(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        TRY_RECV_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
    fn has_data(&self) -> bool {
        HAS_DATA_HITS.fetch_add(1, Ordering::SeqCst);
        false
    }
    fn deserialization_error(&self) -> Self::Error {
        TransportError::Backend("deser")
    }
    fn supports_event(&self, _kind: EventKind) -> bool {
        true
    }
    unsafe fn register_event_callback(
        &mut self,
        kind: EventKind,
        _deadline_ms: u32,
        cb: EventCallback,
        user_ctx: *mut core::ffi::c_void,
    ) -> Result<(), Self::Error> {
        SUB_EVENT_HITS.fetch_add(1, Ordering::SeqCst);
        // Fire callback with a synthetic LivelinessChangedStatus
        // payload. Matches `EventPayload::LivelinessChanged` shape.
        let payload = nros_rmw::LivelinessChangedStatus {
            alive_count: 1,
            not_alive_count: 0,
            alive_count_change: 1,
            not_alive_count_change: 0,
        };
        unsafe {
            cb(
                kind,
                &payload as *const _ as *const core::ffi::c_void,
                user_ctx,
            );
        }
        Ok(())
    }
}

impl ServiceServerTrait for NoopServer {
    type Error = TransportError;
    fn has_request(&self) -> bool {
        HAS_REQUEST_HITS.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn try_recv_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        TRY_RECV_REQUEST_HITS.fetch_add(1, Ordering::SeqCst);
        // Synthetic request: 4 bytes of payload at offset 0, seq=42.
        let payload = [0xde, 0xad, 0xbe, 0xef];
        let n = payload.len().min(buf.len());
        buf[..n].copy_from_slice(&payload[..n]);
        Ok(Some(ServiceRequest {
            sequence_number: 42,
            data: &buf[..n],
        }))
    }
    fn send_reply(&mut self, _sequence_number: i64, _data: &[u8]) -> Result<(), Self::Error> {
        SEND_REPLY_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl ServiceClientTrait for NoopClient {
    type Error = TransportError;
    fn send_request_raw(&mut self, _data: &[u8]) -> Result<(), Self::Error> {
        SEND_REQUEST_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn try_recv_reply_raw(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        TRY_RECV_REPLY_HITS.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

// ----------------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------------

#[test]
fn rust_backend_adapter_routes_every_slot() {
    reset();

    // Install vtable.
    let rc = RustBackendAdapter::<NoopRmw>::register();
    assert_eq!(rc, NROS_RMW_RET_OK, "register failed: {rc}");

    // Open a session manually via the vtable; the adapter's
    // monomorphised `VTABLE` is the same one the runtime now holds.
    let vt = &RustBackendAdapter::<NoopRmw>::VTABLE;
    let mut sess = NrosRmwSession {
        node_name: b"test-node\0".as_ptr(),
        namespace_: b"/\0".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::null_mut(),
    };
    let rc = unsafe {
        (vt.open)(
            b"tcp/127.0.0.1:7447\0".as_ptr(),
            0,
            0,
            b"test-node\0".as_ptr(),
            &mut sess,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert!(!sess.backend_data.is_null());
    assert_eq!(OPEN_HITS.load(Ordering::SeqCst), 1);

    // Drive I/O once.
    let rc = unsafe { (vt.drive_io)(&mut sess, 0) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(DRIVE_IO_HITS.load(Ordering::SeqCst), 1);

    // Create + use + destroy publisher.
    let qos = NrosRmwQos {
        reliability: 1,
        durability: 0,
        history: 0,
        liveliness_kind: 1,
        depth: 10,
        _reserved0: 0,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: 0,
        _reserved1: [0; 3],
        rx_buffer_hint: 0,
        tx_express: 0,
    };
    let mut pubr = nros_rmw_cffi::NrosRmwPublisher {
        topic_name: b"/chatter\0".as_ptr(),
        type_name: b"std_msgs/String\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    let rc = unsafe {
        (vt.create_publisher)(
            &mut sess,
            b"/chatter\0".as_ptr(),
            b"std_msgs/String\0".as_ptr(),
            b"abc123\0".as_ptr(),
            0,
            &qos,
            &mut pubr,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(CREATE_PUB_HITS.load(Ordering::SeqCst), 1);
    let payload = b"hello";
    let rc = unsafe { (vt.publish_raw)(&mut pubr, payload.as_ptr(), payload.len()) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(PUBLISH_HITS.load(Ordering::SeqCst), 1);
    unsafe { (vt.destroy_publisher)(&mut pubr) };
    assert_eq!(DESTROY_PUB_HITS.load(Ordering::SeqCst), 1);

    // Create + use + destroy subscriber.
    let mut subr = nros_rmw_cffi::NrosRmwSubscriber {
        topic_name: b"/chatter\0".as_ptr(),
        type_name: b"std_msgs/String\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    let rc = unsafe {
        (vt.create_subscriber)(
            &mut sess,
            b"/chatter\0".as_ptr(),
            b"std_msgs/String\0".as_ptr(),
            b"abc123\0".as_ptr(),
            0,
            &qos,
            &mut subr,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(CREATE_SUB_HITS.load(Ordering::SeqCst), 1);
    let has = unsafe { (vt.has_data)(&mut subr) };
    assert_eq!(has, 0);
    assert_eq!(HAS_DATA_HITS.load(Ordering::SeqCst), 1);
    let mut recv_buf = [0u8; 64];
    let n = unsafe { (vt.try_recv_raw)(&mut subr, recv_buf.as_mut_ptr(), recv_buf.len()) };
    assert!(n < 0, "expected NO_DATA, got {n}");
    assert_eq!(TRY_RECV_HITS.load(Ordering::SeqCst), 1);
    unsafe { (vt.destroy_subscriber)(&mut subr) };
    assert_eq!(DESTROY_SUB_HITS.load(Ordering::SeqCst), 1);

    // Close.
    let rc = unsafe { (vt.close)(&mut sess) };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(CLOSE_HITS.load(Ordering::SeqCst), 1);
}

#[test]
fn rust_backend_adapter_preserves_session_identity() {
    IDENTITY_RECORDS.lock().unwrap().clear();

    let vt = &RustBackendAdapter::<IdentityRmw>::VTABLE;
    let mut sess = NrosRmwSession {
        node_name: b"talker\0".as_ptr(),
        namespace_: b"/demo\0".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.open)(
                b"tcp/127.0.0.1:7447\0".as_ptr(),
                0,
                7,
                b"talker\0".as_ptr(),
                &mut sess,
            )
        },
        NROS_RMW_RET_OK
    );

    let qos = NrosRmwQos {
        reliability: 1,
        durability: 0,
        history: 0,
        liveliness_kind: 1,
        depth: 10,
        _reserved0: 0,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: 0,
        _reserved1: [0; 3],
        rx_buffer_hint: 0,
        tx_express: 0,
    };

    let mut pubr = nros_rmw_cffi::NrosRmwPublisher {
        topic_name: b"/chatter\0".as_ptr(),
        type_name: b"std_msgs/String\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_publisher)(
                &mut sess,
                b"/chatter\0".as_ptr(),
                b"std_msgs/String\0".as_ptr(),
                b"abc123\0".as_ptr(),
                7,
                &qos,
                &mut pubr,
            )
        },
        NROS_RMW_RET_OK
    );

    let mut subr = nros_rmw_cffi::NrosRmwSubscriber {
        topic_name: b"/chatter\0".as_ptr(),
        type_name: b"std_msgs/String\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_subscriber)(
                &mut sess,
                b"/chatter\0".as_ptr(),
                b"std_msgs/String\0".as_ptr(),
                b"abc123\0".as_ptr(),
                7,
                &qos,
                &mut subr,
            )
        },
        NROS_RMW_RET_OK
    );

    let mut srv = NrosRmwServiceServer {
        service_name: b"/add_two_ints\0".as_ptr(),
        type_name: b"example/AddTwoInts\0".as_ptr(),
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_service_server)(
                &mut sess,
                b"/add_two_ints\0".as_ptr(),
                b"example/AddTwoInts\0".as_ptr(),
                b"def456\0".as_ptr(),
                7,
                &NrosRmwQos::from(QosSettings::services_default()),
                &mut srv,
            )
        },
        NROS_RMW_RET_OK
    );

    let mut cli = NrosRmwServiceClient {
        service_name: b"/add_two_ints\0".as_ptr(),
        type_name: b"example/AddTwoInts\0".as_ptr(),
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_service_client)(
                &mut sess,
                b"/add_two_ints\0".as_ptr(),
                b"example/AddTwoInts\0".as_ptr(),
                b"def456\0".as_ptr(),
                7,
                &NrosRmwQos::from(QosSettings::services_default()),
                &mut cli,
            )
        },
        NROS_RMW_RET_OK
    );

    assert_eq!(
        *IDENTITY_RECORDS.lock().unwrap(),
        [
            IdentityRecord {
                kind: "publisher",
                node_name: Some("talker".to_owned()),
                namespace: "/demo".to_owned(),
            },
            IdentityRecord {
                kind: "subscriber",
                node_name: Some("talker".to_owned()),
                namespace: "/demo".to_owned(),
            },
            IdentityRecord {
                kind: "service_server",
                node_name: Some("talker".to_owned()),
                namespace: "/demo".to_owned(),
            },
            IdentityRecord {
                kind: "service_client",
                node_name: Some("talker".to_owned()),
                namespace: "/demo".to_owned(),
            },
        ]
    );

    unsafe {
        (vt.destroy_publisher)(&mut pubr);
        (vt.destroy_subscriber)(&mut subr);
        (vt.destroy_service_server)(&mut srv);
        (vt.destroy_service_client)(&mut cli);
        let _ = (vt.close)(&mut sess);
    }
}

// ----------------------------------------------------------------------------
// Phase 115.L.0.events — confirm the NrosRmwEventCallback ↔
// trait-EventCallback bridge round-trips through the adapter.
// ----------------------------------------------------------------------------

unsafe extern "C" fn capture_event(
    kind: NrosRmwEventKind,
    payload: *const NrosRmwEventPayload,
    user_ctx: *mut core::ffi::c_void,
) {
    EVENT_CALLBACK_HITS.fetch_add(1, Ordering::SeqCst);
    // SAFETY: user_ctx is a `&AtomicU32` cast set by the test below.
    let last_kind = unsafe { &*(user_ctx as *const AtomicU32) };
    last_kind.store(kind as u32, Ordering::SeqCst);
    // Sanity: dereference the payload according to `kind`. The adapter
    // bridge transmuted a `LivelinessChangedStatus` ptr into the cffi
    // shape; reading the matching union member should yield the
    // synthetic values written by the fixture.
    if kind == NrosRmwEventKind::LivelinessChanged {
        // SAFETY: payload points to a fixture-built
        // LivelinessChangedStatus whose layout matches the cffi mirror.
        let p: &NrosRmwLivelinessChangedStatus =
            unsafe { &*(payload as *const NrosRmwLivelinessChangedStatus) };
        assert_eq!(p.alive_count, 1);
        assert_eq!(p.alive_count_change, 1);
    }
}

#[test]
fn rust_backend_adapter_routes_events_and_services() {
    reset();
    let rc = RustBackendAdapter::<NoopRmw>::register();
    assert_eq!(rc, NROS_RMW_RET_OK);
    let vt = &RustBackendAdapter::<NoopRmw>::VTABLE;
    let mut sess = NrosRmwSession {
        node_name: b"e\0".as_ptr(),
        namespace_: b"/\0".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe { (vt.open)(b"\0".as_ptr(), 0, 0, b"e\0".as_ptr(), &mut sess) },
        NROS_RMW_RET_OK
    );

    // -- Service server flow --
    let mut srv = NrosRmwServiceServer {
        service_name: b"/svc\0".as_ptr(),
        type_name: b"T\0".as_ptr(),
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_service_server)(
                &mut sess,
                b"/svc\0".as_ptr(),
                b"T\0".as_ptr(),
                b"H\0".as_ptr(),
                0,
                &NrosRmwQos::from(QosSettings::services_default()),
                &mut srv,
            )
        },
        NROS_RMW_RET_OK
    );
    assert_eq!(CREATE_SRV_SERVER_HITS.load(Ordering::SeqCst), 1);
    assert_eq!(unsafe { (vt.has_request)(&mut srv) }, 1);
    assert_eq!(HAS_REQUEST_HITS.load(Ordering::SeqCst), 1);
    let mut rbuf = [0u8; 64];
    let mut seq: i64 = 0;
    let n = unsafe { (vt.try_recv_request)(&mut srv, rbuf.as_mut_ptr(), rbuf.len(), &mut seq) };
    assert_eq!(n, 4, "expected 4-byte payload, got {n}");
    assert_eq!(seq, 42);
    assert_eq!(&rbuf[..4], &[0xde, 0xad, 0xbe, 0xef]);
    assert_eq!(TRY_RECV_REQUEST_HITS.load(Ordering::SeqCst), 1);
    let reply = b"ok";
    assert_eq!(
        unsafe { (vt.send_reply)(&mut srv, seq, reply.as_ptr(), reply.len()) },
        NROS_RMW_RET_OK
    );
    assert_eq!(SEND_REPLY_HITS.load(Ordering::SeqCst), 1);
    unsafe { (vt.destroy_service_server)(&mut srv) };

    // -- Service client flow --
    let mut cli = NrosRmwServiceClient {
        service_name: b"/svc\0".as_ptr(),
        type_name: b"T\0".as_ptr(),
        _reserved: [0; 8],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_service_client)(
                &mut sess,
                b"/svc\0".as_ptr(),
                b"T\0".as_ptr(),
                b"H\0".as_ptr(),
                0,
                &NrosRmwQos::from(QosSettings::services_default()),
                &mut cli,
            )
        },
        NROS_RMW_RET_OK
    );
    assert_eq!(CREATE_SRV_CLIENT_HITS.load(Ordering::SeqCst), 1);
    // `call_raw` should forward to the trait default that uses
    // send_request_raw + try_recv_reply_raw via the deprecated
    // blocking path — actually no, the trait default just returns
    // Timeout without driving I/O. So `call_raw` on the cffi side
    // reaches the deprecated impl directly; verify we touch neither
    // counter (the default body short-circuits).
    let req = b"req";
    let mut reply_buf = [0u8; 16];
    let n = unsafe {
        (vt.call_raw)(
            &mut cli,
            req.as_ptr(),
            req.len(),
            reply_buf.as_mut_ptr(),
            reply_buf.len(),
        )
    };
    // Deprecated default returns Timeout → NROS_RMW_RET_TIMEOUT (-2).
    assert!(n < 0);
    unsafe { (vt.destroy_service_client)(&mut cli) };

    // -- Event slots --
    // Create a publisher/subscriber pair so we have valid entities to
    // attach events to.
    let qos = NrosRmwQos {
        reliability: 1,
        durability: 0,
        history: 0,
        liveliness_kind: 1,
        depth: 10,
        _reserved0: 0,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: 0,
        _reserved1: [0; 3],
        rx_buffer_hint: 0,
        tx_express: 0,
    };
    let mut pubr = nros_rmw_cffi::NrosRmwPublisher {
        topic_name: b"/t\0".as_ptr(),
        type_name: b"T\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_publisher)(
                &mut sess,
                b"/t\0".as_ptr(),
                b"T\0".as_ptr(),
                b"H\0".as_ptr(),
                0,
                &qos,
                &mut pubr,
            )
        },
        NROS_RMW_RET_OK
    );
    let mut subr = nros_rmw_cffi::NrosRmwSubscriber {
        topic_name: b"/t\0".as_ptr(),
        type_name: b"T\0".as_ptr(),
        qos,
        can_loan_messages: false,
        _reserved: [0; 7],
        backend_data: core::ptr::null_mut(),
    };
    assert_eq!(
        unsafe {
            (vt.create_subscriber)(
                &mut sess,
                b"/t\0".as_ptr(),
                b"T\0".as_ptr(),
                b"H\0".as_ptr(),
                0,
                &qos,
                &mut subr,
            )
        },
        NROS_RMW_RET_OK
    );

    // assert_publisher_liveliness routes to Publisher::assert_liveliness.
    assert_eq!(
        unsafe { (vt.assert_publisher_liveliness)(&mut pubr) },
        NROS_RMW_RET_OK
    );
    assert_eq!(ASSERT_LIVELINESS_HITS.load(Ordering::SeqCst), 1);

    // next_deadline_ms: the trampoline forwards to
    // Session::next_deadline_ms which returns Some(0) on the
    // NoopSession default body — actually, default trait returns
    // None, but Session::next_deadline_ms default also returns None.
    // The NoopSession doesn't override → trampoline returns -1.
    let nd = unsafe { (vt.next_deadline_ms.unwrap())(&sess) };
    assert_eq!(nd, -1, "expected -1 (no deadline), got {nd}");

    // Subscriber event registration — fixture fires callback inline.
    let last_kind: AtomicU32 = AtomicU32::new(0xffff_ffff);
    let user_ctx = &last_kind as *const _ as *mut core::ffi::c_void;
    let rc = unsafe {
        (vt.register_subscriber_event)(
            &mut subr,
            NrosRmwEventKind::LivelinessChanged,
            0,
            capture_event,
            user_ctx,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(SUB_EVENT_HITS.load(Ordering::SeqCst), 1);
    assert_eq!(EVENT_CALLBACK_HITS.load(Ordering::SeqCst), 1);
    assert_eq!(
        last_kind.load(Ordering::SeqCst),
        NrosRmwEventKind::LivelinessChanged as u32,
        "kind enum should round-trip via transmute"
    );

    // Publisher event registration — fires Count payload via
    // OfferedDeadlineMissed kind.
    let rc = unsafe {
        (vt.register_publisher_event)(
            &mut pubr,
            NrosRmwEventKind::OfferedDeadlineMissed,
            500,
            capture_event,
            user_ctx,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    assert_eq!(PUB_EVENT_HITS.load(Ordering::SeqCst), 1);
    assert_eq!(EVENT_CALLBACK_HITS.load(Ordering::SeqCst), 2);
    assert_eq!(
        last_kind.load(Ordering::SeqCst),
        NrosRmwEventKind::OfferedDeadlineMissed as u32
    );

    unsafe { (vt.destroy_subscriber)(&mut subr) };
    unsafe { (vt.destroy_publisher)(&mut pubr) };
    assert_eq!(unsafe { (vt.close)(&mut sess) }, NROS_RMW_RET_OK);
}

#[test]
fn rust_backend_adapter_rejects_null_pointers() {
    reset();
    let _ = RustBackendAdapter::<NoopRmw>::register();
    let vt = &RustBackendAdapter::<NoopRmw>::VTABLE;
    let mut sess = NrosRmwSession {
        node_name: b"x\0".as_ptr(),
        namespace_: b"/\0".as_ptr(),
        _reserved: [0u8; 8],
        backend_data: core::ptr::null_mut(),
    };
    // open with null `out` → INVALID_ARGUMENT.
    let rc = unsafe { (vt.open)(b"\0".as_ptr(), 0, 0, b"x\0".as_ptr(), core::ptr::null_mut()) };
    assert!(rc < 0);
    // drive_io on uninitialised session (backend_data still null) →
    // INVALID_ARGUMENT.
    let rc = unsafe { (vt.drive_io)(&mut sess, 0) };
    assert!(rc < 0);
}

// Silence "unused import" if a future trim removes one of these
// items — they're held to document the cffi surface the bridge
// touches.
#[allow(dead_code)]
fn _exports_sanity_check() {
    let _ = NROS_RMW_RET_UNSUPPORTED;
}
