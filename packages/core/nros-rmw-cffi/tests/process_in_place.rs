//! Phase 231 Wave 1 (RFC-0038) — `process_raw_in_place` vtable slot routing.
//!
//! Exercises the two new in-place slots via a hand-written stub backend:
//!
//! - `subscription_supports_in_place` → `CffiSubscription::supports_process_in_place()`
//!   (cached at creation).
//! - `process_raw_in_place` → `CffiSubscription::process_raw_in_place(f)`: the stub
//!   hands canned CDR bytes to the marshalled Rust closure, then reports no-data.
//! - A vtable leaving both slots `None` → `supports_*` is false and the in-place
//!   call surfaces an `Err` (the runtime then uses the buffered path).
//!
//! Hermetic — no real backend, no zenohd. The CFFI marshalling
//! (`run_process_in_place` trampoline + the capability cache) is what's tested.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicI32, Ordering},
};
use std::sync::Mutex;

use nros_rmw::{QosSettings, RmwConfig, Session, SessionMode, Subscription, TopicInfo};
use nros_rmw_cffi::{
    CffiRmw, CffiSubscription, NROS_RMW_RET_NO_DATA, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos,
    NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

// Serialize register→open→assert so the two tests don't race on the shared
// "default" registry slot + the global script atomics.
static GUARD: Mutex<()> = Mutex::new(());

const CANNED: &[u8] = &[0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
static SUPPORTS: AtomicI32 = AtomicI32::new(1);
static TAKE_REMAINING: AtomicI32 = AtomicI32::new(0);

// ---- stub vtable slots ----

unsafe extern "C" fn stub_open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = std::ptr::dangling_mut::<c_void>();
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_create_publisher(
    _: *mut NrosRmwSession,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::nros_rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) {}
unsafe extern "C" fn stub_publish_raw(
    _: *mut NrosRmwPublisher,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_subscription(
    _: *mut NrosRmwSession,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::nros_rmw_subscription_options_t,
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x99usize as *mut c_void;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) {}
unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscription, _: *mut u8, _: usize) -> i32 {
    NROS_RMW_RET_NO_DATA
}
unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscription) -> i32 {
    0
}
unsafe extern "C" fn stub_create_service(
    _: *mut NrosRmwSession,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_service(_: *mut NrosRmwService) {}
unsafe extern "C" fn stub_try_recv_request(
    _: *mut NrosRmwService,
    _: *mut u8,
    _: usize,
    _: *mut i64,
) -> i32 {
    0
}
unsafe extern "C" fn stub_has_request(_: *mut NrosRmwService) -> i32 {
    0
}
unsafe extern "C" fn stub_send_reply(
    _: *mut NrosRmwService,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_client(
    _: *mut NrosRmwSession,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) {}
unsafe extern "C" fn stub_reg_sub_event(
    _: *mut NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut core::ffi::c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_reg_pub_event(
    _: *mut NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut core::ffi::c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_assert_liveliness(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
// ---- the slots under test ----

unsafe extern "C" fn scripted_supports(_: *mut NrosRmwSubscription) -> i32 {
    SUPPORTS.load(Ordering::SeqCst)
}

unsafe extern "C" fn scripted_process(
    _: *mut NrosRmwSubscription,
    ctx: *mut c_void,
    cb: Option<unsafe extern "C" fn(ctx: *mut c_void, ptr: *const u8, len: usize)>,
) -> i32 {
    let cb = cb.expect("vtable slot");
    if TAKE_REMAINING.fetch_sub(1, Ordering::SeqCst) > 0 {
        unsafe { cb(ctx, CANNED.as_ptr(), CANNED.len()) };
        1
    } else {
        NROS_RMW_RET_NO_DATA
    }
}

fn base_vtable() -> NrosRmwVtable {
    NrosRmwVtable {
        create_session: Some(stub_open),
        destroy_session: Some(stub_close),
        drive_io: Some(stub_drive_io),
        create_publisher: Some(stub_create_publisher),
        destroy_publisher: Some(stub_destroy_publisher),
        publish_raw: Some(stub_publish_raw),
        create_subscription: Some(stub_create_subscription),
        destroy_subscription: Some(stub_destroy_subscription),
        try_recv_raw: Some(stub_try_recv_raw),
        has_data: Some(stub_has_data),
        create_service: Some(stub_create_service),
        destroy_service: Some(stub_destroy_service),
        try_recv_request: Some(stub_try_recv_request),
        has_request: Some(stub_has_request),
        send_reply: Some(stub_send_reply),
        create_client: Some(stub_create_client),
        destroy_client: Some(stub_destroy_client),
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscription_event: Some(stub_reg_sub_event),
        register_publisher_event: Some(stub_reg_pub_event),
        assert_publisher_liveliness: Some(stub_assert_liveliness),
        next_deadline_ms: None,
        set_wake_callback: None,
        pub_loan: None,
        pub_commit: None,
        pub_discard: None,
        sub_borrow: None,
        sub_release: None,
        service_server_available: None,
        try_recv_sequence: None,
        publish_streamed: None,
        ping_session: None,
        subscription_supports_in_place: None,
        process_raw_in_place: None,
    }
}

fn open_subscriber(topic: &str) -> CffiSubscription {
    use nros_rmw::Rmw;
    let mut session = CffiRmw
        .open(&RmwConfig {
            locator: "tcp/127.0.0.1:7447",
            mode: SessionMode::Client,
            domain_id: 0,
            node_name: "stub_node",
            namespace: "/",
            properties: &[],
        })
        .expect("open");
    let info = TopicInfo::new(topic, "example/Stub", "RIHS01_stub");
    let sub = session
        .create_subscription(&info, QosSettings::default())
        .expect("create_subscription");
    // Leak the session: its `close` would drop through the stub vtable whose
    // `backend_data` is a bare sentinel, not a `Box`. The process exits after.
    core::mem::forget(session);
    sub
}

#[test]
fn in_place_delivers_bytes_then_drains() {
    let _g = GUARD.lock().unwrap();
    SUPPORTS.store(1, Ordering::SeqCst);
    TAKE_REMAINING.store(1, Ordering::SeqCst);

    let mut vt = base_vtable();
    vt.subscription_supports_in_place = Some(scripted_supports);
    vt.process_raw_in_place = Some(scripted_process);
    let vt: &'static NrosRmwVtable = Box::leak(Box::new(vt));
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let mut sub = open_subscriber("/inplace_ok");

    // Capability cached from the slot at creation.
    assert!(
        Subscription::supports_process_in_place(&sub),
        "supports_process_in_place should be true when the slot reports 1"
    );

    // The marshalled closure receives the canned bytes in place.
    let mut captured: Vec<u8> = Vec::new();
    let r = Subscription::process_raw_in_place(&mut sub, |raw| captured.extend_from_slice(raw));
    assert!(r.unwrap(), "first take should process a message");
    assert_eq!(
        captured, CANNED,
        "in-place bytes must match what the slot delivered"
    );

    // Second take: slot reports no-data → Ok(false), callback not invoked.
    let r2 = Subscription::process_raw_in_place(&mut sub, |_| {
        panic!("callback must not fire on no-data")
    });
    assert!(!r2.unwrap(), "drained subscriber should report no message");
}

#[test]
fn in_place_unsupported_when_slots_null() {
    let _g = GUARD.lock().unwrap();
    let vt: &'static NrosRmwVtable = Box::leak(Box::new(base_vtable())); // both slots None
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let mut sub = open_subscriber("/inplace_null");
    assert!(
        !Subscription::supports_process_in_place(&sub),
        "NULL capability slot → unsupported"
    );
    assert!(
        Subscription::process_raw_in_place(&mut sub, |_| {}).is_err(),
        "NULL process slot → Err (runtime falls back to buffered)"
    );
}
