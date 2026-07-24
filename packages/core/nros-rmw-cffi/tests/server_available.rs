//! Phase 124.C.4 — `CffiClient::server_available()` routing test.
//!
//! Exercises the new vtable slot via a stub backend that toggles the
//! return code through `0` → `1` → `-NROS_RMW_RET_ERROR`. Verifies that:
//!
//! - A backend leaving `service_server_available` as `None` surfaces
//!   `Err(TransportError::Unsupported)` to the caller.
//! - Slot returning `0` → `Ok(false)`, slot returning `1` → `Ok(true)`.
//! - Slot returning a negative `nros_rmw_ret_t` → `Err(_)` (any
//!   transport-level variant — the exact mapping is owned by
//!   `error_from_ret`).
//!
//! The test runs against a hand-written stub vtable. No real backend
//! needed — the routing logic in `CffiClient` is what's under
//! test.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicI32, Ordering},
};

use nros_rmw::{
    ClientTrait, QosSettings, RmwConfig, ServiceInfo, Session, SessionMode, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwClient,
    NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwService, NrosRmwSession, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

// ---- Mutable script the stub reads on each `server_available` call ----

static SCRIPT: AtomicI32 = AtomicI32::new(0);

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
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x42usize as *mut c_void;
    }
    NROS_RMW_RET_OK
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
// The slot under test: returns whatever `SCRIPT` currently holds.
unsafe extern "C" fn scripted_server_available(_: *mut NrosRmwClient) -> i32 {
    SCRIPT.load(Ordering::SeqCst)
}

static VTABLE_WITH_SLOT: NrosRmwVtable = NrosRmwVtable {
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
    service_server_available: Some(scripted_server_available),
    try_recv_sequence: None,
    publish_streamed: None,
    ping_session: None,
    subscription_supports_in_place: None,
    process_raw_in_place: None,
};

static VTABLE_NULL_SLOT: NrosRmwVtable = NrosRmwVtable {
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
};

fn open_client(svc_name: &str) -> nros_rmw_cffi::CffiClient {
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
    let info = ServiceInfo::new(svc_name, "example/Stub", "RIHS01_stub");
    let client = session
        .create_client(&info, QosSettings::services_default())
        .expect("create_client");
    // Leak the session intentionally — its `close` would try to drop
    // through the stub vtable, and the stub's `backend_data` is a
    // bare integer, not a `Box`. The test process exits right after.
    core::mem::forget(session);
    client
}

#[test]
fn server_available_unsupported_when_slot_null() {
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), &VTABLE_NULL_SLOT) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let client = open_client("/svc_null_slot");
    match client.server_available() {
        Err(TransportError::Unsupported) => {}
        other => panic!("expected Err(Unsupported), got {other:?}"),
    }
}

#[test]
fn server_available_tracks_slot_return_value() {
    let ret = unsafe { nros_rmw_cffi_register_named(c"default".as_ptr(), &VTABLE_WITH_SLOT) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let client = open_client("/svc_scripted");

    SCRIPT.store(0, Ordering::SeqCst);
    assert!(!client.server_available().unwrap());

    SCRIPT.store(1, Ordering::SeqCst);
    assert!(client.server_available().unwrap());

    SCRIPT.store(NROS_RMW_RET_ERROR, Ordering::SeqCst);
    assert!(client.server_available().is_err());

    // Backends sometimes report ≥ 1 as a participant count rather
    // than a strict boolean. Treat any positive non-1 value as
    // "available" — covered in `CffiClient::server_available`.
    SCRIPT.store(7, Ordering::SeqCst);
    assert!(client.server_available().unwrap());
}

#[test]
fn vtable_has_slot_field() {
    // Compile-time check that the new field exists in the C ABI;
    // the const initialisers above already enforce structural
    // presence, but assert against an explicit `Option<fn>` value
    // for documentation.
    let _ = VTABLE_WITH_SLOT.service_server_available.is_some();
    let _ = VTABLE_NULL_SLOT.service_server_available.is_none();
}
