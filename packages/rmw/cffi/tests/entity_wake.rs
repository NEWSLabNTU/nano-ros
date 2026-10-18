//! Issue 1940 — an entity-scoped wake must reach the backend through the cffi
//! route, and a backend that cannot wake must SAY so.
//!
//! Before the fix `CffiSubscription` / `CffiService` / `CffiClient` inherited
//! `register_waker`'s empty default, and the C ABI had no per-entity slot to
//! forward to. So `rclc_polling_*_set_wake_callback`, the C++ twins and Rust's
//! `recv().await` accepted a callback, answered OK, and nothing ever fired it.
//!
//! Two backends, the two roads a backend reaches this crate by:
//!
//! - a C-shaped STUB vtable (XRCE / Cyclone / uORB road) whose
//!   `*_set_wake` slots record what they were handed, so the test can play the
//!   backend's arrival path by calling the recorded callback;
//! - a Rust backend behind `RustBackendAdapter` (zenoh's road) whose
//!   `register_waker` keeps the waker ONE-SHOT, as `atomic-waker` does, so the
//!   test also proves the adapter re-arms it per take.
//!
//! Every positive assertion fails with the fix reverted: `register_waker`
//! returned `()` and installed nothing, so no callback was recorded and none
//! fired.

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
    c_waker::{CWakeState, make_waker},
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_INVALID_ARGUMENT, NROS_RMW_RET_OK, NrosRmwClient,
    NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, RustBackendAdapter, generated,
    nros_rmw_cffi_register_named,
};

type WakeCb = generated::rmw_entity_wake_callback_t;

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

// ===========================================================================
// Part 1 — a C-shaped backend with per-entity wake slots
// ===========================================================================

/// What a `*_set_wake` slot was last handed, per entity kind.
struct Installed {
    cb: AtomicPtr<()>,
    ctx: AtomicPtr<c_void>,
    token: AtomicU32,
    calls: AtomicU32,
}

impl Installed {
    const fn new() -> Self {
        Self {
            cb: AtomicPtr::new(core::ptr::null_mut()),
            ctx: AtomicPtr::new(core::ptr::null_mut()),
            token: AtomicU32::new(0),
            calls: AtomicU32::new(0),
        }
    }

    fn record(&self, cb: WakeCb, ctx: *mut c_void, token: u32) -> NrosRmwRet {
        self.cb.store(
            cb.map_or(core::ptr::null_mut(), |f| f as *mut ()),
            Ordering::SeqCst,
        );
        self.ctx.store(ctx, Ordering::SeqCst);
        self.token.store(token, Ordering::SeqCst);
        self.calls.fetch_add(1, Ordering::SeqCst);
        NROS_RMW_RET_OK
    }

    /// Play the backend's arrival path: call whatever was installed.
    fn arrive(&self) {
        let cb = self.cb.load(Ordering::SeqCst);
        assert!(
            !cb.is_null(),
            "the backend was never handed a wake callback"
        );
        let f: unsafe extern "C" fn(*mut c_void, u32) = unsafe { core::mem::transmute(cb) };
        unsafe {
            f(
                self.ctx.load(Ordering::SeqCst),
                self.token.load(Ordering::SeqCst),
            )
        };
    }
}

static SUB_SLOT: Installed = Installed::new();
static SRV_SLOT: Installed = Installed::new();
static CLI_SLOT: Installed = Installed::new();

unsafe extern "C" fn stub_subscription_set_wake(
    s: *mut NrosRmwSubscription,
    cb: WakeCb,
    ctx: *mut c_void,
    token: u32,
) -> NrosRmwRet {
    if s.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SUB_SLOT.record(cb, ctx, token)
}

unsafe extern "C" fn stub_service_set_wake(
    s: *mut NrosRmwService,
    cb: WakeCb,
    ctx: *mut c_void,
    token: u32,
) -> NrosRmwRet {
    if s.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SRV_SLOT.record(cb, ctx, token)
}

unsafe extern "C" fn stub_client_set_wake(
    c: *mut NrosRmwClient,
    cb: WakeCb,
    ctx: *mut c_void,
    token: u32,
) -> NrosRmwRet {
    if c.is_null() {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    CLI_SLOT.record(cb, ctx, token)
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

/// The mandatory slots, with NO per-entity wake: what XRCE and uORB look like.
static POLL_ONLY_VTABLE: NrosRmwVtable = NrosRmwVtable {
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
    ..EMPTY_VTABLE
};

/// The same backend WITH the three slots.
static WAKING_VTABLE: NrosRmwVtable = NrosRmwVtable {
    subscription_set_wake: Some(stub_subscription_set_wake),
    service_set_wake: Some(stub_service_set_wake),
    client_set_wake: Some(stub_client_set_wake),
    ..POLL_ONLY_VTABLE
};

/// A C caller's callback (`rclc_polling_*_set_wake_callback`'s `cb`).
unsafe extern "C" fn count_wake(ctx: *mut c_void) {
    unsafe { (*(ctx as *const AtomicU32)).fetch_add(1, Ordering::SeqCst) };
}

/// A Rust future's waker — not a C waker, so it needs the owned-cell road.
struct FlagWaker(AtomicUsize);

impl Wake for FlagWaker {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// One entity kind's round trip on the C-shaped backend: a C waker and a Rust
/// waker each reach the slot, and the backend's call reaches each of them.
fn c_backend_round_trip(slot: &Installed, register: &dyn Fn(&Waker) -> bool) {
    // ---- the C road: the waker `rclc_polling_*_set_wake_callback` builds ----
    let fired = Box::leak(Box::new(AtomicU32::new(0)));
    let state = Box::leak(Box::new(CWakeState {
        fn_ptr: Some(count_wake),
        ctx: fired as *const AtomicU32 as *mut c_void,
    }));
    let c_waker = unsafe { make_waker(state) };

    assert!(
        register(&c_waker),
        "a backend with the slot must accept the wake"
    );
    assert_eq!(
        slot.calls.load(Ordering::SeqCst),
        1,
        "the slot must be called"
    );
    assert_eq!(
        slot.token.load(Ordering::SeqCst),
        nros_rmw_cffi::generated::NROS_RMW_WAKE_TOKEN_UNKNOWN as u32,
        "a caller with no entity attribution passes the UNKNOWN token"
    );
    slot.arrive();
    slot.arrive();
    assert_eq!(
        fired.load(Ordering::SeqCst),
        2,
        "each arrival must reach the C callback"
    );

    // Re-registering the same waker (a future does it every poll) must not
    // re-enter the backend.
    assert!(register(&c_waker));
    assert_eq!(
        slot.calls.load(Ordering::SeqCst),
        1,
        "same ctx, no second install"
    );

    // ---- the Rust road: an arbitrary waker, carried in an owned cell ----
    let flag = Arc::new(FlagWaker(AtomicUsize::new(0)));
    let rust_waker = Waker::from(flag.clone());
    assert!(register(&rust_waker));
    assert_eq!(
        slot.calls.load(Ordering::SeqCst),
        2,
        "a new ctx is installed"
    );
    slot.arrive();
    assert_eq!(
        flag.0.load(Ordering::SeqCst),
        1,
        "the arrival must wake the future"
    );
    // One-shot, as a future expects: it re-registers on its next poll.
    slot.arrive();
    assert_eq!(flag.0.load(Ordering::SeqCst), 1);
    assert!(register(&rust_waker));
    assert_eq!(
        slot.calls.load(Ordering::SeqCst),
        2,
        "the cell is reused, not reinstalled"
    );
    slot.arrive();
    assert_eq!(flag.0.load(Ordering::SeqCst), 2);
}

/// Serialises the two C-backend tests: they share the slot recorders.
static C_BACKEND_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn every_cffi_entity_forwards_its_wake_to_the_backend_slot() {
    let _g = C_BACKEND_LOCK.lock().unwrap();
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ew_waking".as_ptr(), &WAKING_VTABLE) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_waking", &config("ew_node")).expect("open");

    let sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    c_backend_round_trip(&SUB_SLOT, &|w| Subscription::register_waker(&sub, w));

    let srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    c_backend_round_trip(&SRV_SLOT, &|w| ServiceTrait::register_waker(&srv, w));

    let cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    c_backend_round_trip(&CLI_SLOT, &|w| ClientTrait::register_waker(&cli, w));
}

#[test]
fn a_backend_without_the_slot_reports_the_wake_unsupported() {
    let _g = C_BACKEND_LOCK.lock().unwrap();
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ew_poll_only".as_ptr(), &POLL_ONLY_VTABLE) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_poll_only", &config("ew_poll")).expect("open");
    let flag = Arc::new(FlagWaker(AtomicUsize::new(0)));
    let waker = Waker::from(flag);

    let sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    assert!(
        !Subscription::register_waker(&sub, &waker),
        "no slot, no wake: say so"
    );
    let srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    assert!(!ServiceTrait::register_waker(&srv, &waker));
    let cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    assert!(!ClientTrait::register_waker(&cli, &waker));
}

// ===========================================================================
// Part 2 — a Rust backend behind `RustBackendAdapter` (zenoh's road)
// ===========================================================================

/// Per-kind state of the Rust backend: the ONE-SHOT waker it was given (taken
/// on arrival, as `atomic-waker` does) and whether it has data to report.
struct RustSide {
    waker: Mutex<Option<Waker>>,
    /// Samples queued and not yet taken.
    queued: AtomicU32,
    registrations: AtomicU32,
}

impl RustSide {
    const fn new() -> Self {
        Self {
            waker: Mutex::new(None),
            queued: AtomicU32::new(0),
            registrations: AtomicU32::new(0),
        }
    }

    fn pending(&self) -> bool {
        self.queued.load(Ordering::SeqCst) > 0
    }

    /// Take one queued sample.
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
        self.registrations.fetch_add(1, Ordering::SeqCst);
        *self.waker.lock().unwrap() = Some(w.clone());
        true
    }

    /// The backend's arrival hook: queue a sample, wake and FORGET the waker.
    fn arrive(&self) {
        self.queued.fetch_add(1, Ordering::SeqCst);
        if let Some(w) = self.waker.lock().unwrap().take() {
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

/// A Rust backend that leaves `register_waker` at the trait default.
#[derive(Default)]
struct DeafRmw;
struct DeafSession;
struct DeafSub;

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
    type ServiceHandle = WakeSrv;
    type ClientHandle = WakeCli;

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
    fn create_service(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<WakeSrv, Self::Error> {
        Err(TransportError::Unsupported)
    }
    fn create_client(&mut self, _: &ServiceInfo, _: QoSProfile) -> Result<WakeCli, Self::Error> {
        Err(TransportError::Unsupported)
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

/// C wake state + counter for one entity, leaked for the test's lifetime as
/// `rclc_wake_state_t` storage would be.
fn c_wake() -> (&'static AtomicU32, Waker) {
    let fired = Box::leak(Box::new(AtomicU32::new(0)));
    let state = Box::leak(Box::new(CWakeState {
        fn_ptr: Some(count_wake),
        ctx: fired as *const AtomicU32 as *mut c_void,
    }));
    (fired, unsafe { make_waker(state) })
}

#[test]
fn a_c_wake_reaches_a_rust_backend_through_the_adapter_and_survives_each_take() {
    assert_eq!(
        unsafe { RustBackendAdapter::<WakeRmw>::register_named(c"ew_rust".as_ptr()) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_rust", &config("ew_rust")).expect("open");
    let mut buf = [0u8; 8];

    // ---- subscription ----
    let mut sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    let (fired, waker) = c_wake();
    assert!(Subscription::register_waker(&sub, &waker));
    assert_eq!(
        R_SUB.registrations.load(Ordering::SeqCst),
        1,
        "the backend got a waker"
    );

    R_SUB.arrive();
    assert_eq!(
        fired.load(Ordering::SeqCst),
        1,
        "arrival must reach the C callback"
    );
    // The backend FORGOT its waker (one-shot), so the take must hand it back.
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), Some(1));
    assert_eq!(
        R_SUB.registrations.load(Ordering::SeqCst),
        2,
        "the take re-arms"
    );
    R_SUB.arrive();
    assert_eq!(
        fired.load(Ordering::SeqCst),
        2,
        "the wake persists across takes"
    );

    // The window: a sample lands while the waker is spent (after the last
    // wake, before the consumer's take re-armed it). Nothing reaches the
    // callback then — so the re-arm must notice the sample and fire itself.
    R_SUB.arrive(); // 2 queued, waker spent
    assert_eq!(fired.load(Ordering::SeqCst), 2);
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), Some(1)); // 1 left
    assert_eq!(
        fired.load(Ordering::SeqCst),
        3,
        "re-arm with data waiting must fire"
    );
    assert_eq!(sub.take_serialized(&mut buf).unwrap(), Some(1)); // drained
    assert_eq!(
        fired.load(Ordering::SeqCst),
        3,
        "re-arm on an empty queue must not fire"
    );
    R_SUB.arrive();
    assert_eq!(fired.load(Ordering::SeqCst), 4);

    // ---- service ----
    let mut srv = session
        .create_service(&service(), QoSProfile::default())
        .expect("service");
    let (s_fired, s_waker) = c_wake();
    assert!(ServiceTrait::register_waker(&srv, &s_waker));
    R_SRV.arrive();
    assert_eq!(s_fired.load(Ordering::SeqCst), 1);
    assert!(srv.take_request(&mut buf).unwrap().is_some());
    R_SRV.arrive();
    assert_eq!(
        s_fired.load(Ordering::SeqCst),
        2,
        "re-armed by take_request"
    );

    // ---- client ----
    let mut cli = session
        .create_client(&service(), QoSProfile::default())
        .expect("client");
    let (c_fired, c_waker) = c_wake();
    assert!(ClientTrait::register_waker(&cli, &c_waker));
    R_CLI.arrive();
    assert_eq!(c_fired.load(Ordering::SeqCst), 1);
    assert!(cli.take_response_raw(&mut buf).unwrap().is_some());
    R_CLI.arrive();
    assert_eq!(
        c_fired.load(Ordering::SeqCst),
        2,
        "re-armed by take_response"
    );
}

#[test]
fn a_rust_backend_that_cannot_wake_answers_unsupported_through_the_adapter() {
    // The slot is non-NULL for every Rust backend; the HANDLE decides.
    let vt = &RustBackendAdapter::<DeafRmw>::VTABLE;
    assert!(vt.subscription_set_wake.is_some());
    assert_eq!(
        unsafe { RustBackendAdapter::<DeafRmw>::register_named(c"ew_deaf".as_ptr()) },
        NROS_RMW_RET_OK
    );
    let mut session = CffiRmw::open_with_rmw("ew_deaf", &config("ew_deaf")).expect("open");
    let sub = session
        .create_subscription(&topic(), QoSProfile::default())
        .expect("subscription");
    let (_fired, waker) = c_wake();
    assert!(
        !Subscription::register_waker(&sub, &waker),
        "a backend on the trait default must not be reported as waking"
    );
}
