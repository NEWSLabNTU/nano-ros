//! Issue 1551 (defect 2) — an exhausted heap at a create trampoline is a
//! RETURNED `NROS_RMW_RET_BAD_ALLOC`, never libstd's OOM abort.
//!
//! The adapter used to `Box::new` every handle a backend returned. On an
//! exhausted heap that called `handle_alloc_error`, and on a Zephyr
//! `native_sim` image libstd's OOM hook then wrote to fd 2, which is issue
//! 0589's `zvfs_write` recursion: a stack-overflow SIGSEGV with no message.
//!
//! This file is its own test binary so it can install a global allocator that
//! refuses exactly one request SIZE on demand. Every handle type here has a
//! distinct odd size no other allocation in the process makes, so the refusal
//! lands on the adapter's handle box and nowhere else. Before the fix, each
//! `BAD_ALLOC` assertion below was a process abort ("memory allocation of N
//! bytes failed"), not a test failure.

#![cfg(feature = "alloc")]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{
    ClientTrait, Publisher, QoSProfile, Rmw, RmwConfig, ServiceInfo, ServiceRequest, ServiceTrait,
    Session, Subscription, TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    NROS_RMW_RET_BAD_ALLOC, NROS_RMW_RET_OK, NrosRmwNode, NrosRmwPublisher, NrosRmwQos,
    NrosRmwSession, RustBackendAdapter,
    generated::{
        rmw_client_t, rmw_message_type_support_t, rmw_service_t, rmw_service_type_support_t,
        rmw_subscription_t,
    },
};

// ----------------------------------------------------------------------------
// An allocator that refuses one size on demand.
// ----------------------------------------------------------------------------

/// 0 = refuse nothing.
static REFUSE_SIZE: AtomicUsize = AtomicUsize::new(0);

struct RefusingAlloc;

unsafe impl GlobalAlloc for RefusingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let refuse = REFUSE_SIZE.load(Ordering::SeqCst);
        if refuse != 0 && layout.size() == refuse {
            return core::ptr::null_mut();
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: RefusingAlloc = RefusingAlloc;

// ----------------------------------------------------------------------------
// A backend whose handles have distinct, unusual sizes and count their drops.
// ----------------------------------------------------------------------------

static SESSION_DROPS: AtomicUsize = AtomicUsize::new(0);
static SESSION_CLOSES: AtomicUsize = AtomicUsize::new(0);
static PUB_DROPS: AtomicUsize = AtomicUsize::new(0);
static SUB_DROPS: AtomicUsize = AtomicUsize::new(0);
static SRV_DROPS: AtomicUsize = AtomicUsize::new(0);
static CLI_DROPS: AtomicUsize = AtomicUsize::new(0);

macro_rules! sized_handle {
    ($name:ident, $bytes:expr, $drops:ident) => {
        struct $name {
            _pad: [u8; $bytes],
        }
        impl $name {
            fn new() -> Self {
                Self { _pad: [0; $bytes] }
            }
        }
        impl Drop for $name {
            fn drop(&mut self) {
                $drops.fetch_add(1, Ordering::SeqCst);
            }
        }
    };
}

sized_handle!(OomSession, 4091, SESSION_DROPS);
sized_handle!(OomPublisher, 4093, PUB_DROPS);
sized_handle!(OomSubscriber, 4097, SUB_DROPS);
sized_handle!(OomServer, 4099, SRV_DROPS);
sized_handle!(OomClient, 4111, CLI_DROPS);

#[derive(Default)]
struct OomRmw;

impl Rmw for OomRmw {
    type Session = OomSession;
    type Error = TransportError;
    fn open(self, _config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        Ok(OomSession::new())
    }
}

impl Session for OomSession {
    type Error = TransportError;
    type PublisherHandle = OomPublisher;
    type SubscriptionHandle = OomSubscriber;
    type ServiceHandle = OomServer;
    type ClientHandle = OomClient;

    fn create_publisher(
        &mut self,
        _topic: &TopicInfo,
        _qos: QoSProfile,
    ) -> Result<Self::PublisherHandle, Self::Error> {
        Ok(OomPublisher::new())
    }
    fn create_subscription(
        &mut self,
        _topic: &TopicInfo,
        _qos: QoSProfile,
    ) -> Result<Self::SubscriptionHandle, Self::Error> {
        Ok(OomSubscriber::new())
    }
    fn create_service(
        &mut self,
        _service: &ServiceInfo,
        _qos: QoSProfile,
    ) -> Result<Self::ServiceHandle, Self::Error> {
        Ok(OomServer::new())
    }
    fn create_client(
        &mut self,
        _service: &ServiceInfo,
        _qos: QoSProfile,
    ) -> Result<Self::ClientHandle, Self::Error> {
        Ok(OomClient::new())
    }
    fn close(&mut self) -> Result<(), Self::Error> {
        SESSION_CLOSES.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Publisher for OomPublisher {
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

impl Subscription for OomSubscriber {
    type Error = TransportError;
    fn take_serialized(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        Ok(None)
    }
    fn deserialization_error(&self) -> Self::Error {
        TransportError::DeserializationError
    }
}

impl ServiceTrait for OomServer {
    type Error = TransportError;
    fn has_request(&self) -> bool {
        false
    }
    fn take_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        Ok(None)
    }
    fn send_response(&mut self, _sequence_number: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ClientTrait for OomClient {
    type Error = TransportError;
    fn send_request_raw(&mut self, _data: &[u8]) -> Result<i64, Self::Error> {
        Ok(0)
    }
    fn take_response_raw(&mut self, _buf: &mut [u8]) -> Result<Option<(usize, i64)>, Self::Error> {
        Ok(None)
    }
}

// ----------------------------------------------------------------------------
// The test.
// ----------------------------------------------------------------------------

fn zeroed<T>() -> T {
    // SAFETY: every type zeroed here is a `repr(C)` bindgen struct of raw
    // pointers, integers and bools, for which all-zero is a valid value.
    unsafe { core::mem::zeroed() }
}

/// Refuse the next allocation of `size_of::<T>()` bytes while `f` runs.
fn refusing<T, R>(f: impl FnOnce() -> R) -> R {
    REFUSE_SIZE.store(core::mem::size_of::<T>(), Ordering::SeqCst);
    let r = f();
    REFUSE_SIZE.store(0, Ordering::SeqCst);
    r
}

/// ONE test, deliberately: `REFUSE_SIZE` is process-global and `cargo test`
/// runs a file's tests as threads of one process.
#[test]
fn an_exhausted_heap_at_every_create_trampoline_is_bad_alloc_not_an_abort() {
    let vt = &RustBackendAdapter::<OomRmw>::VTABLE;
    let locator = c"tcp/127.0.0.1:7447";
    let name = c"oom_probe";
    let type_name = c"std_msgs/msg/Int32";
    let topic = c"/oom";

    // --- session -----------------------------------------------------------
    let create_session = vt.create_session.expect("create_session slot");
    let destroy_session = vt.destroy_session.expect("destroy_session slot");
    let mut refused_session: NrosRmwSession = zeroed();
    let rc = refusing::<OomSession, _>(|| unsafe {
        create_session(
            locator.as_ptr(),
            0,
            0,
            name.as_ptr(),
            core::ptr::null(),
            &mut refused_session,
        )
    });
    assert_eq!(rc, NROS_RMW_RET_BAD_ALLOC, "unboxable session");
    assert!(refused_session.backend_data.is_null());
    assert_eq!(
        (
            SESSION_CLOSES.load(Ordering::SeqCst),
            SESSION_DROPS.load(Ordering::SeqCst)
        ),
        (1, 1),
        "an unboxable session is closed and dropped, never left open unheld"
    );

    let mut session: NrosRmwSession = zeroed();
    let rc = unsafe {
        create_session(
            locator.as_ptr(),
            0,
            0,
            name.as_ptr(),
            core::ptr::null(),
            &mut session,
        )
    };
    assert_eq!(
        rc, NROS_RMW_RET_OK,
        "negative control: a heap that has room"
    );
    assert!(!session.backend_data.is_null());

    let mut node: NrosRmwNode = zeroed();
    node.name = name.as_ptr();
    node.session = &mut session;
    let qos: NrosRmwQos = zeroed();
    let msg_ts = rmw_message_type_support_t {
        type_name: type_name.as_ptr(),
        type_hash: core::ptr::null(),
    };
    let srv_ts = rmw_service_type_support_t {
        type_name: c"example_interfaces/srv/AddTwoInts".as_ptr(),
        type_hash: core::ptr::null(),
    };

    // --- publisher ---------------------------------------------------------
    let create_publisher = vt.create_publisher.expect("create_publisher slot");
    let mut publisher: NrosRmwPublisher = zeroed();
    let rc = refusing::<OomPublisher, _>(|| unsafe {
        create_publisher(
            &node,
            &msg_ts,
            topic.as_ptr(),
            0,
            &qos,
            core::ptr::null(),
            &mut publisher,
        )
    });
    assert_eq!(rc, NROS_RMW_RET_BAD_ALLOC, "unboxable publisher");
    assert!(publisher.backend_data.is_null());
    assert_eq!(
        PUB_DROPS.load(Ordering::SeqCst),
        1,
        "the backend's publisher is dropped (undeclared), not leaked"
    );
    let rc = unsafe {
        create_publisher(
            &node,
            &msg_ts,
            topic.as_ptr(),
            0,
            &qos,
            core::ptr::null(),
            &mut publisher,
        )
    };
    assert_eq!(rc, NROS_RMW_RET_OK);
    unsafe { vt.destroy_publisher.expect("slot")(&mut publisher) };
    assert_eq!(PUB_DROPS.load(Ordering::SeqCst), 2);

    // --- subscription ------------------------------------------------------
    let create_subscription = vt.create_subscription.expect("create_subscription slot");
    let mut sub: rmw_subscription_t = zeroed();
    let rc = refusing::<OomSubscriber, _>(|| unsafe {
        create_subscription(
            &node,
            &msg_ts,
            topic.as_ptr(),
            0,
            &qos,
            core::ptr::null(),
            &mut sub,
        )
    });
    assert_eq!(rc, NROS_RMW_RET_BAD_ALLOC, "unboxable subscription");
    assert!(sub.backend_data.is_null());
    assert_eq!(SUB_DROPS.load(Ordering::SeqCst), 1);

    // --- service server ----------------------------------------------------
    let create_service = vt.create_service.expect("create_service slot");
    let mut srv: rmw_service_t = zeroed();
    let rc = refusing::<OomServer, _>(|| unsafe {
        create_service(&node, &srv_ts, c"/add".as_ptr(), 0, &qos, &mut srv)
    });
    assert_eq!(rc, NROS_RMW_RET_BAD_ALLOC, "unboxable service server");
    assert!(srv.backend_data.is_null());
    assert_eq!(SRV_DROPS.load(Ordering::SeqCst), 1);

    // --- service client ----------------------------------------------------
    let create_client = vt.create_client.expect("create_client slot");
    let mut cli: rmw_client_t = zeroed();
    let rc = refusing::<OomClient, _>(|| unsafe {
        create_client(&node, &srv_ts, c"/add".as_ptr(), 0, &qos, &mut cli)
    });
    assert_eq!(rc, NROS_RMW_RET_BAD_ALLOC, "unboxable service client");
    assert!(cli.backend_data.is_null());
    assert_eq!(CLI_DROPS.load(Ordering::SeqCst), 1);

    let rc = unsafe { destroy_session(&mut session) };
    assert_eq!(rc, NROS_RMW_RET_OK);
}
