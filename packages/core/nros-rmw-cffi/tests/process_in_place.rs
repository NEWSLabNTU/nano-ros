//! Phase 231 Wave 1 (RFC-0038) — `process_raw_in_place` vtable slot routing.
//!
//! Exercises the two new in-place slots via a hand-written stub backend:
//!
//! - `subscriber_supports_in_place` → `CffiSubscriber::supports_process_in_place()`
//!   (cached at creation).
//! - `process_raw_in_place` → `CffiSubscriber::process_raw_in_place(f)`: the stub
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

use nros_rmw::{QosSettings, RmwConfig, Session, SessionMode, Subscriber, TopicInfo};
use nros_rmw_cffi::{
    CffiRmw, CffiSubscriber, NROS_RMW_RET_NO_DATA, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwServiceClient, NrosRmwServiceServer, NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable,
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
    _: *const u8,
    _: u8,
    _: u32,
    _: *const u8,
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
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
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
unsafe extern "C" fn stub_create_subscriber(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwSubscriber,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x99usize as *mut c_void;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_subscriber(_: *mut NrosRmwSubscriber) {}
unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscriber, _: *mut u8, _: usize) -> i32 {
    NROS_RMW_RET_NO_DATA
}
unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscriber) -> i32 {
    0
}
unsafe extern "C" fn stub_create_service_server(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwServiceServer,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_service_server(_: *mut NrosRmwServiceServer) {}
unsafe extern "C" fn stub_try_recv_request(
    _: *mut NrosRmwServiceServer,
    _: *mut u8,
    _: usize,
    _: *mut i64,
) -> i32 {
    0
}
unsafe extern "C" fn stub_has_request(_: *mut NrosRmwServiceServer) -> i32 {
    0
}
unsafe extern "C" fn stub_send_reply(
    _: *mut NrosRmwServiceServer,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_service_client(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwServiceClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_service_client(_: *mut NrosRmwServiceClient) {}
unsafe extern "C" fn stub_call_raw(
    _: *mut NrosRmwServiceClient,
    _: *const u8,
    _: usize,
    _: *mut u8,
    _: usize,
) -> i32 {
    -1
}
unsafe extern "C" fn stub_reg_sub_event(
    _: *mut NrosRmwSubscriber,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_reg_pub_event(
    _: *mut NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_assert_liveliness(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

// ---- the slots under test ----

unsafe extern "C" fn scripted_supports(_: *mut NrosRmwSubscriber) -> i32 {
    SUPPORTS.load(Ordering::SeqCst)
}

unsafe extern "C" fn scripted_process(
    _: *mut NrosRmwSubscriber,
    ctx: *mut c_void,
    cb: unsafe extern "C" fn(ctx: *mut c_void, ptr: *const u8, len: usize),
) -> i32 {
    if TAKE_REMAINING.fetch_sub(1, Ordering::SeqCst) > 0 {
        unsafe { cb(ctx, CANNED.as_ptr(), CANNED.len()) };
        1
    } else {
        NROS_RMW_RET_NO_DATA
    }
}

fn base_vtable() -> NrosRmwVtable {
    NrosRmwVtable {
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscriber_event: stub_reg_sub_event,
        register_publisher_event: stub_reg_pub_event,
        assert_publisher_liveliness: stub_assert_liveliness,
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
        subscriber_supports_in_place: None,
        process_raw_in_place: None,
    }
}

fn open_subscriber(topic: &str) -> CffiSubscriber {
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
        .create_subscriber(&info, QosSettings::default())
        .expect("create_subscriber");
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
    vt.subscriber_supports_in_place = Some(scripted_supports);
    vt.process_raw_in_place = Some(scripted_process);
    let vt: &'static NrosRmwVtable = Box::leak(Box::new(vt));
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let mut sub = open_subscriber("/inplace_ok");

    // Capability cached from the slot at creation.
    assert!(
        Subscriber::supports_process_in_place(&sub),
        "supports_process_in_place should be true when the slot reports 1"
    );

    // The marshalled closure receives the canned bytes in place.
    let mut captured: Vec<u8> = Vec::new();
    let r = Subscriber::process_raw_in_place(&mut sub, |raw| captured.extend_from_slice(raw));
    assert_eq!(r.unwrap(), true, "first take should process a message");
    assert_eq!(
        captured, CANNED,
        "in-place bytes must match what the slot delivered"
    );

    // Second take: slot reports no-data → Ok(false), callback not invoked.
    let r2 =
        Subscriber::process_raw_in_place(&mut sub, |_| panic!("callback must not fire on no-data"));
    assert_eq!(
        r2.unwrap(),
        false,
        "drained subscriber should report no message"
    );
}

#[test]
fn in_place_unsupported_when_slots_null() {
    let _g = GUARD.lock().unwrap();
    let vt: &'static NrosRmwVtable = Box::leak(Box::new(base_vtable())); // both slots None
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let mut sub = open_subscriber("/inplace_null");
    assert!(
        !Subscriber::supports_process_in_place(&sub),
        "NULL capability slot → unsupported"
    );
    assert!(
        Subscriber::process_raw_in_place(&mut sub, |_| {}).is_err(),
        "NULL process slot → Err (runtime falls back to buffered)"
    );
}
