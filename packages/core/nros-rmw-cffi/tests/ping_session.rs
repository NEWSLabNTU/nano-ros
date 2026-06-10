//! Phase 124.F.4 — `CffiSession::ping_session` routing test.
//!
//! Three scenarios:
//!
//! 1. **Native slot — peer responds.** Backend returns `RET_OK`.
//!    `CffiSession::ping_session` should map this to `Ok(())`.
//! 2. **Native slot — timeout.** Backend returns `RET_TIMEOUT`.
//!    Mapped to `Err(TransportError::Timeout)`.
//! 3. **NULL slot — backend can't probe.** Runtime surfaces
//!    `Err(TransportError::Unsupported)` without dispatch.
//!
//! Tests run against hand-written stub vtables. No real transport
//! needed — the routing logic in `CffiSession::ping_session` is
//! what's under test.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicI32, AtomicUsize, Ordering},
};

use nros_rmw::{RmwConfig, Session, SessionMode, TransportError};
use nros_rmw_cffi::{
    NROS_RMW_RET_OK, NROS_RMW_RET_TIMEOUT, NROS_RMW_RET_UNSUPPORTED, NrosRmwEventCallback,
    NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwServiceClient,
    NrosRmwServiceServer, NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

// ---- Scripted return value + call counter ---------------------------------

static PING_SCRIPT: AtomicI32 = AtomicI32::new(0);
static PING_CALLS: AtomicUsize = AtomicUsize::new(0);

// ---- Stub vtable boilerplate ----------------------------------------------

unsafe extern "C" fn stub_open(
    _: *const u8,
    _: u8,
    _: u32,
    _: *const u8,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
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
    _: *mut NrosRmwSubscriber,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_subscriber(_: *mut NrosRmwSubscriber) {}
unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscriber, _: *mut u8, _: usize) -> i32 {
    0
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

// Scripted ping slot.
unsafe extern "C" fn scripted_ping(_: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    PING_CALLS.fetch_add(1, Ordering::SeqCst);
    PING_SCRIPT.load(Ordering::SeqCst)
}

const fn base_vtable() -> NrosRmwVtable {
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

static VTABLE_WITH_PING: NrosRmwVtable = {
    let mut v = base_vtable();
    v.ping_session = Some(scripted_ping);
    v
};

static VTABLE_NO_PING: NrosRmwVtable = base_vtable();

fn open_session(name: &str, vt: &'static NrosRmwVtable) -> nros_rmw_cffi::CffiSession {
    let cname = format!("{name}\0");
    let ret = unsafe { nros_rmw_cffi_register_named(cname.as_ptr() as *const _, vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);
    nros_rmw_cffi::CffiSession::open_named(
        name,
        "tcp/127.0.0.1:7447",
        SessionMode::Client as u8,
        0,
        "stub_node",
    )
    .expect("open_named")
}

#[test]
fn ping_session_ok_when_slot_returns_ok() {
    PING_CALLS.store(0, Ordering::SeqCst);
    PING_SCRIPT.store(NROS_RMW_RET_OK, Ordering::SeqCst);
    let mut sess = open_session("tb_ping_ok", &VTABLE_WITH_PING);
    sess.ping_session(50).expect("ping should succeed");
    assert_eq!(PING_CALLS.load(Ordering::SeqCst), 1);
    core::mem::forget(sess);
}

#[test]
fn ping_session_timeout_when_slot_returns_timeout() {
    PING_CALLS.store(0, Ordering::SeqCst);
    PING_SCRIPT.store(NROS_RMW_RET_TIMEOUT, Ordering::SeqCst);
    let mut sess = open_session("tb_ping_timeout", &VTABLE_WITH_PING);
    match sess.ping_session(10) {
        Err(TransportError::Timeout) => {}
        other => panic!("expected Err(Timeout), got {other:?}"),
    }
    assert_eq!(PING_CALLS.load(Ordering::SeqCst), 1);
    core::mem::forget(sess);
}

#[test]
fn ping_session_unsupported_when_slot_null() {
    PING_CALLS.store(0, Ordering::SeqCst);
    let mut sess = open_session("tb_ping_null", &VTABLE_NO_PING);
    match sess.ping_session(10) {
        Err(TransportError::Unsupported) => {}
        other => panic!("expected Err(Unsupported), got {other:?}"),
    }
    assert_eq!(
        PING_CALLS.load(Ordering::SeqCst),
        0,
        "NULL slot must short-circuit before dispatch"
    );
    core::mem::forget(sess);
}

// Silence dead-code warnings — `RmwConfig` and friends are
// exercised via `open_named` but not directly.
#[allow(dead_code)]
fn _silence(_: RmwConfig<'_>) {}
