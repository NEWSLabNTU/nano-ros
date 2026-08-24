//! Phase 124.C.4 — `CffiClient::server_available()` routing test.
//!
//! Exercises the new vtable slot via a stub backend that toggles the
//! return code through `0` → `1` → `-NROS_RMW_RET_ERROR`. Verifies that:
//!
//! - A backend leaving `service_server_is_available` as `None` surfaces
//!   `Err(TransportError::Unsupported)` to the caller.
//! - Slot returning `0` → `Ok(false)`, slot returning `1` → `Ok(true)`.
//! - Slot returning a negative `rmw_ret_t` → `Err(_)` (any
//!   transport-level variant — the exact mapping is owned by
//!   `error_from_ret`).
//!
//! The test runs against a hand-written stub vtable. No real backend
//! needed — the routing logic in `CffiClient` is what's under
//! test.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
};

use nros_rmw::{
    ClientTrait, QosSettings, RmwConfig, ServiceInfo, Session, SessionMode, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos,
    NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

// ---- Mutable script the stub reads on each `server_available` call ----

static SCRIPT: AtomicI32 = AtomicI32::new(0);
/// What the slot writes to `*out_available` when it returns OK. Phase 376 W3.d
/// step A split this from `SCRIPT`: the status and the answer are now two
/// values, which is the whole point of the change.
static AVAIL: AtomicBool = AtomicBool::new(false);
/// Makes the stub violate the contract — write the out-parameter and THEN
/// return an error — so the runtime's handling of a misbehaving backend is
/// tested rather than the stub's own good manners.
static SCRIPT_WRITES_ON_ERROR: AtomicBool = AtomicBool::new(false);

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
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_publish_raw(
    _: *const NrosRmwPublisher,
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
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take(
    _: *const NrosRmwSubscription,
    _: *mut u8,
    _: usize,
    _: *mut usize,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_data(
    _: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_data = false };
    NROS_RMW_RET_OK
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
unsafe extern "C" fn stub_destroy_service(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take_request(
    _: *const NrosRmwService,
    _: *mut u8,
    _: usize,
    _: *mut i64,
    _: *mut usize,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_request(
    _: *mut NrosRmwService,
    out_has_request: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_request = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_send_reply(
    _: *const NrosRmwService,
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
unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_reg_sub_event(
    _: *const NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut core::ffi::c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_reg_pub_event(
    _: *const NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut core::ffi::c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_assert_liveliness(_: *const NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
// The slot under test: returns whatever `SCRIPT` currently holds.
unsafe extern "C" fn scripted_server_is_available(
    _: *const NrosRmwClient,
    out_available: *mut bool,
) -> NrosRmwRet {
    let rc = SCRIPT.load(Ordering::SeqCst);
    if rc == NROS_RMW_RET_OK || SCRIPT_WRITES_ON_ERROR.load(Ordering::SeqCst) {
        // SAFETY: the caller owns a `bool`; the runtime always passes one.
        unsafe { *out_available = AVAIL.load(Ordering::SeqCst) };
    }
    // On a non-OK status the out-parameter is left ALONE — asserted below.
    rc
}

static VTABLE_WITH_SLOT: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish_raw),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_reply),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    subscription_event_init: Some(stub_reg_sub_event),
    publisher_event_init: Some(stub_reg_pub_event),
    publisher_assert_liveliness: Some(stub_assert_liveliness),
    service_server_is_available: Some(scripted_server_is_available),
    ..EMPTY_VTABLE
};

static VTABLE_NULL_SLOT: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish_raw),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_reply),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    subscription_event_init: Some(stub_reg_sub_event),
    publisher_event_init: Some(stub_reg_pub_event),
    publisher_assert_liveliness: Some(stub_assert_liveliness),
    ..EMPTY_VTABLE
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

    SCRIPT.store(NROS_RMW_RET_OK, Ordering::SeqCst);
    AVAIL.store(false, Ordering::SeqCst);
    assert!(!client.server_available().unwrap());

    AVAIL.store(true, Ordering::SeqCst);
    assert!(client.server_available().unwrap());

    SCRIPT.store(NROS_RMW_RET_ERROR, Ordering::SeqCst);
    assert!(client.server_available().is_err());

    // The case this shape RETIRES: the old slot multiplexed a count and a
    // status through one `int32_t`, so a backend reporting a participant count
    // of 7 had to be read as "available", and the runtime carried an arm for
    // "any positive value other than 1". With the answer in a `bool` there is
    // no non-spec value left to be lenient about — which is exactly what makes
    // upstream's `RMW_RET_ERROR = 1` adoptable in step B.

    // And the property the old shape could not express: on a non-OK status the
    // caller's answer must not be readable as fresh.
    //
    // Asserted against the RUNTIME, not against the stub. The obvious version
    // of this test — call the stub and check it left `*out_available` alone —
    // asserts only that the stub in this file does what its own body says,
    // which is worth nothing. What matters is that a backend VIOLATING the
    // contract cannot leak a value into a caller: `SCRIPT_WRITES_ON_ERROR`
    // makes the stub write `true` and then fail.
    SCRIPT_WRITES_ON_ERROR.store(true, Ordering::SeqCst);
    SCRIPT.store(NROS_RMW_RET_ERROR, Ordering::SeqCst);
    AVAIL.store(true, Ordering::SeqCst);
    assert!(
        client.server_available().is_err(),
        "a backend that writes the out-param AND fails must still surface the error"
    );
    SCRIPT_WRITES_ON_ERROR.store(false, Ordering::SeqCst);
}

#[test]
fn vtable_has_slot_field() {
    // Compile-time check that the new field exists in the C ABI;
    // the const initialisers above already enforce structural
    // presence, but assert against an explicit `Option<fn>` value
    // for documentation.
    let _ = VTABLE_WITH_SLOT.service_server_is_available.is_some();
    let _ = VTABLE_NULL_SLOT.service_server_is_available.is_none();
}
