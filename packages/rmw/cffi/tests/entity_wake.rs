//! Issue 1940 — the Rust half of per-entity wake through `nros-rmw-cffi`.
//!
//! `tests/run/entity_wake_callback.c` covers the C caller on a C stub backend
//! (three kinds, three arrivals each, plus `UNSUPPORTED`), and
//! `nros-rmw-zenoh`'s `cffi_smoke` covers a live subscription. What neither
//! reaches, and this file does:
//!
//! 1. a RUST waker (a future's, not a `c_waker`) registered on a
//!    `CffiSubscription` / `CffiService` / `CffiClient` over a C-shaped
//!    backend that fills upstream's three `*_set_on_new_*_callback` slots;
//! 2. service and client over `RustBackendAdapter` (zenoh's road) whose
//!    backend keeps its waker FIRE-ONCE, as `atomic-waker` does;
//! 3. a sample that lands while the consumer's callback is still running,
//!    i.e. after the backend spent its waker and before the consumer could
//!    do anything about it — the adapter must already have re-armed;
//! 4. a Rust backend left on the trait default (`register_waker` -> `false`)
//!    reports UNSUPPORTED through the adapter, not OK.
//!
//! Each test was mutation-checked against the code it names: (1) and (2) fail
//! with `runtime_entity_wake` made a no-op, (3) fails with the re-arm in
//! `Hosted::waker_wake` removed, (4) fails with the trait default answering
//! `true`.

#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicPtr, AtomicU32, AtomicUsize, Ordering},
};
use std::{
    sync::{Arc, Mutex},
    task::{Wake, Waker},
};

use nros_rmw::{
    ClientTrait, QoSPolicyMask, QoSProfile, Rmw, RmwConfig, ServiceInfo, ServiceRequest,
    ServiceTrait, Session, SessionMode, Subscription, TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, NrosRmwClient,
    NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, RustBackendAdapter, generated,
    nros_rmw_cffi_register_named,
};

fn config(node: &'static str) -> RmwConfig<'static> {
    RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name: node,
        namespace: "",
        properties: &[],
    }
}

fn topic() -> TopicInfo<'static> {
    TopicInfo::new("/ew", "std_msgs/msg/Int32", "RIHS01_ew")
}

fn service() -> ServiceInfo<'static> {
    ServiceInfo::new("/ew_srv", "example_interfaces/srv/AddTwoInts", "RIHS01_ew")
}

/// A future's waker: counts wakes, and is NOT a `c_waker`.
struct FlagWaker(AtomicUsize);

impl FlagWaker {
    fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(0)))
    }
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl Wake for FlagWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

// ===========================================================================
// Part 1 — a C-shaped backend filling upstream's three slots
// ===========================================================================

/// What a `*_set_on_new_*_callback` slot stored for one entity:
/// `(callback, user_data)`, as the header says a backend keeps it.
struct Installed {
    cb: AtomicPtr<()>,
    user_data: AtomicPtr<c_void>,
}

impl Installed {
    const fn new() -> Self {
        Self {
            cb: AtomicPtr::new(core::ptr::null_mut()),
            user_data: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    fn store(&self, cb: generated::rmw_event_callback_t, user_data: *const c_void) -> NrosRmwRet {
        self.cb.store(
            cb.map_or(core::ptr::null_mut(), |f| f as *mut ()),
            Ordering::SeqCst,
        );
        self.user_data
            .store(user_data as *mut c_void, Ordering::SeqCst);
        NROS_RMW_RET_OK
    }

    /// The backend's arrival path: one sample, `number_of_events = 1`.
    fn arrive(&self) {
        let cb = self.cb.load(Ordering::SeqCst);
        assert!(!cb.is_null(), "the backend was never handed a callback");
        // SAFETY: `cb` was stored from a `rmw_event_callback_t`.
        let f: unsafe extern "C" fn(*const c_void, usize) = unsafe { core::mem::transmute(cb) };
        unsafe { f(self.user_data.load(Ordering::SeqCst), 1) };
    }
}

static SUB_SLOT: Installed = Installed::new();
static SRV_SLOT: Installed = Installed::new();
static CLI_SLOT: Installed = Installed::new();

unsafe extern "C" fn stub_subscription_set_on_new_message_callback(
    s: *mut NrosRmwSubscription,
    cb: generated::rmw_event_callback_t,
    user_data: *const c_void,
) -> NrosRmwRet {
    if s.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SUB_SLOT.store(cb, user_data)
}

unsafe extern "C" fn stub_service_set_on_new_request_callback(
    s: *mut NrosRmwService,
    cb: generated::rmw_event_callback_t,
    user_data: *const c_void,
) -> NrosRmwRet {
    if s.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SRV_SLOT.store(cb, user_data)
}

unsafe extern "C" fn stub_client_set_on_new_response_callback(
    c: *mut NrosRmwClient,
    cb: generated::rmw_event_callback_t,
    user_data: *const c_void,
) -> NrosRmwRet {
    if c.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    CLI_SLOT.store(cb, user_data)
}

unsafe extern "C" fn stub_open(
    _locator: *const core::ffi::c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const core::ffi::c_char,
    _options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xE4A1_1000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_close(_session: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_drive_io(_session: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_publisher(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_message_type_support_t,
    _topic_name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xE4A1_2000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_publisher(_p: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_publish(
    _p: *const NrosRmwPublisher,
    _data: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_subscription(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_message_type_support_t,
    _topic_name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::rmw_subscription_options_t,
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xE4A1_3000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_subscription(_s: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_take(
    _s: *const NrosRmwSubscription,
    _span: *mut generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_has_data(_s: *mut NrosRmwSubscription, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_service(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_service_type_support_t,
    _name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    out: *mut NrosRmwService,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xE4A1_4000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_service(_s: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_take_request(
    _s: *const NrosRmwService,
    _span: *mut generated::rmw_mut_byte_span_t,
    _seq: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_has_request(_s: *mut NrosRmwService, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_send_response(
    _s: *const NrosRmwService,
    _seq: i64,
    _data: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_client(
    _node: *const NrosRmwNode,
    _ts: *const generated::rmw_service_type_support_t,
    _name: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xE4A1_5000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_destroy_client(_c: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

static WAKING_VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_response),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    subscription_set_on_new_message_callback: Some(stub_subscription_set_on_new_message_callback),
    service_set_on_new_request_callback: Some(stub_service_set_on_new_request_callback),
    client_set_on_new_response_callback: Some(stub_client_set_on_new_response_callback),
    ..EMPTY_VTABLE
};

/// One entity kind: a Rust waker reaches the slot, and every arrival the
/// backend reports reaches the waker (a registration persists until replaced).
fn rust_waker_round_trip(kind: &str, slot: &Installed, register: &dyn Fn(&Waker) -> bool) {
    let flag = FlagWaker::new();
    let waker = Waker::from(flag.clone());
    assert!(register(&waker), "{kind}: a backend with the slot accepts");
    for n in 1..=3 {
        slot.arrive();
        assert_eq!(flag.count(), n, "{kind}: arrival {n} must wake the future");
    }
}

#[test]
fn a_rust_waker_on_every_cffi_entity_fires_on_arrival() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ew_c".as_ptr(), &WAKING_VTABLE) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_c", &config("ew_c")).expect("open");

    let sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    rust_waker_round_trip("subscription", &SUB_SLOT, &|w| {
        Subscription::register_waker(&sub, w)
    });

    let srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    rust_waker_round_trip("service", &SRV_SLOT, &|w| {
        ServiceTrait::register_waker(&srv, w)
    });

    let cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    rust_waker_round_trip("client", &CLI_SLOT, &|w| {
        ClientTrait::register_waker(&cli, w)
    });
}

// ===========================================================================
// Part 2 — a Rust backend behind `RustBackendAdapter` (zenoh's road)
// ===========================================================================

/// Per-kind state of the Rust backend: a FIRE-ONCE waker (taken on arrival,
/// as `atomic-waker` does) and the samples not yet taken.
struct RustSide {
    waker: Mutex<Option<Waker>>,
    queued: AtomicU32,
}

impl RustSide {
    const fn new() -> Self {
        Self {
            waker: Mutex::new(None),
            queued: AtomicU32::new(0),
        }
    }

    fn pending(&self) -> bool {
        self.queued.load(Ordering::SeqCst) > 0
    }

    fn pop(&self) -> bool {
        let mut n = self.queued.load(Ordering::SeqCst);
        while n > 0 {
            match self
                .queued
                .compare_exchange(n, n - 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return true,
                Err(now) => n = now,
            }
        }
        false
    }

    fn register(&self, w: &Waker) -> bool {
        *self.waker.lock().unwrap() = Some(w.clone());
        true
    }

    /// The backend's arrival hook: queue a sample, wake and FORGET the waker.
    /// The lock is released before waking, so a wake that re-arms (or a
    /// callback that lands another sample) does not deadlock.
    fn arrive(&self) {
        self.queued.fetch_add(1, Ordering::SeqCst);
        let spent = self.waker.lock().unwrap().take();
        if let Some(w) = spent {
            w.wake();
        }
    }
}

static R_SUB: RustSide = RustSide::new();
static R_SRV: RustSide = RustSide::new();
static R_CLI: RustSide = RustSide::new();

#[derive(Default)]
struct WakeRmw;
struct WakeSession;
struct WakeSub;
struct WakeSrv;
struct WakeCli;
struct NoPub;

impl Rmw for WakeRmw {
    type Session = WakeSession;
    type Error = TransportError;
    fn open(self, _config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        Ok(WakeSession)
    }
}

impl Session for WakeSession {
    type Error = TransportError;
    type PublisherHandle = NoPub;
    type SubscriptionHandle = WakeSub;
    type ServiceHandle = WakeSrv;
    type ClientHandle = WakeCli;

    fn create_publisher(&mut self, _: &TopicInfo, _: QoSProfile) -> Result<NoPub, Self::Error> {
        Err(TransportError::Unsupported)
    }
    fn create_subscription(
        &mut self,
        _: &TopicInfo,
        _: QoSProfile,
    ) -> Result<WakeSub, Self::Error> {
        Ok(WakeSub)
    }
    fn create_service(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<WakeSrv, Self::Error> {
        Ok(WakeSrv)
    }
    fn create_client(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<WakeCli, Self::Error> {
        Ok(WakeCli)
    }
    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn supported_qos_policies(&self) -> QoSPolicyMask {
        QoSPolicyMask::CORE
    }
}

impl nros_rmw::Publisher for NoPub {
    type Error = TransportError;
    fn publish_raw(&self, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn buffer_error(&self) -> Self::Error {
        TransportError::BufferTooSmall
    }
    fn serialization_error(&self) -> Self::Error {
        TransportError::Backend("ser")
    }
}

impl Subscription for WakeSub {
    type Error = TransportError;
    fn register_waker(&self, w: &Waker) -> bool {
        R_SUB.register(w)
    }
    fn has_data(&self) -> bool {
        R_SUB.pending()
    }
    fn take_serialized(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        if R_SUB.pop() {
            buf[0] = 7;
            Ok(Some(1))
        } else {
            Ok(None)
        }
    }
    fn deserialization_error(&self) -> Self::Error {
        TransportError::Backend("deser")
    }
}

impl ServiceTrait for WakeSrv {
    type Error = TransportError;
    fn register_waker(&self, w: &Waker) -> bool {
        R_SRV.register(w)
    }
    fn has_request(&self) -> bool {
        R_SRV.pending()
    }
    fn take_request<'a>(
        &mut self,
        buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        if R_SRV.pop() {
            buf[0] = 1;
            Ok(Some(ServiceRequest {
                sequence_number: 1,
                data: &buf[..1],
            }))
        } else {
            Ok(None)
        }
    }
    fn send_response(&mut self, _seq: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ClientTrait for WakeCli {
    type Error = TransportError;
    fn register_waker(&self, w: &Waker) -> bool {
        R_CLI.register(w)
    }
    fn send_request_raw(&mut self, _data: &[u8]) -> Result<i64, Self::Error> {
        Ok(1)
    }
    fn take_response_raw(&mut self, buf: &mut [u8]) -> Result<Option<(usize, i64)>, Self::Error> {
        if R_CLI.pop() {
            buf[0] = 3;
            Ok(Some((1, 1)))
        } else {
            Ok(None)
        }
    }
}

fn open_wake_session(node: &'static str) -> nros_rmw_cffi::CffiSession {
    // Same name, same vtable: a second registration overwrites idempotently,
    // so the two adapter tests may run in either order.
    assert_eq!(
        unsafe { RustBackendAdapter::<WakeRmw>::register_named(c"ew_rust".as_ptr()) },
        NROS_RMW_RET_OK
    );
    CffiRmw::open_with_rmw("ew_rust", &config(node)).expect("open")
}

#[test]
fn service_and_client_through_the_adapter_wake_a_rust_waker_on_every_arrival() {
    let mut session = open_wake_session("ew_rust_sc");
    let mut buf = [0u8; 8];

    // ---- service ----
    let mut srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    let s_flag = FlagWaker::new();
    assert!(ServiceTrait::register_waker(
        &srv,
        &Waker::from(s_flag.clone())
    ));
    R_SRV.arrive();
    assert_eq!(s_flag.count(), 1, "a request must wake the future");
    // The backend spent its fire-once waker on that arrival; the second one
    // only reaches the future if the adapter handed it a fresh one.
    R_SRV.arrive();
    assert_eq!(s_flag.count(), 2, "the second request must wake it too");
    assert!(srv.take_request(&mut buf).unwrap().is_some());
    assert!(srv.take_request(&mut buf).unwrap().is_some());
    assert!(srv.take_request(&mut buf).unwrap().is_none());

    // ---- client ----
    let mut cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    let c_flag = FlagWaker::new();
    assert!(ClientTrait::register_waker(
        &cli,
        &Waker::from(c_flag.clone())
    ));
    R_CLI.arrive();
    assert_eq!(c_flag.count(), 1, "a response must wake the future");
    R_CLI.arrive();
    assert_eq!(c_flag.count(), 2, "the second response must wake it too");
    assert!(cli.take_response_raw(&mut buf).unwrap().is_some());
    assert!(cli.take_response_raw(&mut buf).unwrap().is_some());
    assert!(cli.take_response_raw(&mut buf).unwrap().is_none());
}

/// A Rust waker that lands ONE more sample the first time it is woken — the
/// arrival that races a consumer's callback, made deterministic.
struct ArriveOnFirstWake {
    wakes: AtomicUsize,
}

impl Wake for ArriveOnFirstWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if self.wakes.fetch_add(1, Ordering::SeqCst) == 0 {
            R_SUB.arrive();
        }
    }
}

#[test]
fn a_sample_landing_during_the_callback_still_wakes() {
    let mut session = open_wake_session("ew_rust_sub");
    let mut buf = [0u8; 8];
    let mut sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");

    let w = Arc::new(ArriveOnFirstWake {
        wakes: AtomicUsize::new(0),
    });
    assert!(Subscription::register_waker(&sub, &Waker::from(w.clone())));

    // First arrival: the backend takes (spends) its waker and wakes. Inside
    // that wake a second sample lands, BEFORE the consumer has taken anything
    // or re-polled. Only an adapter that re-armed the backend before calling
    // out turns that second sample into a second wake.
    R_SUB.arrive();
    assert_eq!(
        w.wakes.load(Ordering::SeqCst),
        2,
        "the sample that landed during the callback must wake as well"
    );
    assert!(sub.has_data());
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), Some(1));
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), Some(1));
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), None);

    // And the registration is still live afterwards.
    R_SUB.arrive();
    assert_eq!(w.wakes.load(Ordering::SeqCst), 3);
}

// ===========================================================================
// Part 3 — a Rust backend on the trait default
// ===========================================================================

#[derive(Default)]
struct DeafRmw;
struct DeafSession;
struct DeafSub;
struct DeafSrv;
struct DeafCli;

impl Rmw for DeafRmw {
    type Session = DeafSession;
    type Error = TransportError;
    fn open(self, _config: &RmwConfig) -> Result<Self::Session, Self::Error> {
        Ok(DeafSession)
    }
}

impl Session for DeafSession {
    type Error = TransportError;
    type PublisherHandle = NoPub;
    type SubscriptionHandle = DeafSub;
    type ServiceHandle = DeafSrv;
    type ClientHandle = DeafCli;

    fn create_publisher(&mut self, _: &TopicInfo, _: QoSProfile) -> Result<NoPub, Self::Error> {
        Err(TransportError::Unsupported)
    }
    fn create_subscription(
        &mut self,
        _: &TopicInfo,
        _: QoSProfile,
    ) -> Result<DeafSub, Self::Error> {
        Ok(DeafSub)
    }
    fn create_service(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<DeafSrv, Self::Error> {
        Ok(DeafSrv)
    }
    fn create_client(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<DeafCli, Self::Error> {
        Ok(DeafCli)
    }
    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn drive_io(&mut self, _timeout_ms: i32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn supported_qos_policies(&self) -> QoSPolicyMask {
        QoSPolicyMask::CORE
    }
}

impl Subscription for DeafSub {
    type Error = TransportError;
    fn take_serialized(&mut self, _buf: &mut [u8]) -> Result<Option<usize>, Self::Error> {
        Ok(None)
    }
    fn deserialization_error(&self) -> Self::Error {
        TransportError::Backend("deser")
    }
}

impl ServiceTrait for DeafSrv {
    type Error = TransportError;
    fn take_request<'a>(
        &mut self,
        _buf: &'a mut [u8],
    ) -> Result<Option<ServiceRequest<'a>>, Self::Error> {
        Ok(None)
    }
    fn send_response(&mut self, _seq: i64, _data: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ClientTrait for DeafCli {
    type Error = TransportError;
    fn send_request_raw(&mut self, _data: &[u8]) -> Result<i64, Self::Error> {
        Ok(1)
    }
    fn take_response_raw(&mut self, _buf: &mut [u8]) -> Result<Option<(usize, i64)>, Self::Error> {
        Ok(None)
    }
}

#[test]
fn a_rust_backend_on_the_trait_default_reports_unsupported_through_the_adapter() {
    // The adapter fills all three slots for EVERY Rust backend; the handle's
    // `register_waker` answer is what decides.
    let vt = &RustBackendAdapter::<DeafRmw>::VTABLE;
    assert!(vt.subscription_set_on_new_message_callback.is_some());
    assert!(vt.service_set_on_new_request_callback.is_some());
    assert!(vt.client_set_on_new_response_callback.is_some());

    assert_eq!(
        unsafe { RustBackendAdapter::<DeafRmw>::register_named(c"ew_deaf".as_ptr()) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_deaf", &config("ew_deaf")).expect("open");
    let waker = Waker::from(FlagWaker::new());

    let sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    assert!(
        !Subscription::register_waker(&sub, &waker),
        "subscription: a backend on the trait default must not read as waking"
    );
    let srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    assert!(
        !ServiceTrait::register_waker(&srv, &waker),
        "service: a backend on the trait default must not read as waking"
    );
    let cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    assert!(
        !ClientTrait::register_waker(&cli, &waker),
        "client: a backend on the trait default must not read as waking"
    );
}
