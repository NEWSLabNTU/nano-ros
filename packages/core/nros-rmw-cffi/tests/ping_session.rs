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
    NROS_RMW_RET_OK, NROS_RMW_RET_TIMEOUT, NROS_RMW_RET_UNSUPPORTED, NrosRmwClient,
    NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwService, NrosRmwSession, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

// ---- Scripted return value + call counter ---------------------------------

static PING_SCRIPT: AtomicI32 = AtomicI32::new(0);
static PING_CALLS: AtomicUsize = AtomicUsize::new(0);

// ---- Stub vtable boilerplate ----------------------------------------------

unsafe extern "C" fn stub_open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
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
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) {}
unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscription, _: *mut u8, _: usize) -> i32 {
    0
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
// Scripted ping slot.
unsafe extern "C" fn scripted_ping(_: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    PING_CALLS.fetch_add(1, Ordering::SeqCst);
    PING_SCRIPT.load(Ordering::SeqCst)
}

const fn base_vtable() -> NrosRmwVtable {
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
