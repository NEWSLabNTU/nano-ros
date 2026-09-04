//! phase-206 W3 — backend-specific session properties must SURVIVE the C seam,
//! and a property that cannot survive it must be REFUSED rather than dropped.
//!
//! `RmwConfig::properties` has existed for as long as the Rust RMW trait has,
//! and every backend reads it. Nothing carried it across the C boundary: the
//! runtime handed `create_session` a NULL options pointer, and the Rust-backend
//! adapter on the far side built `properties: &[]` unconditionally, next to a
//! `let _ = options;`. So a C or C++ image could state no transport
//! configuration at all — no zenoh `listen` endpoint, no TLS certificate, no
//! scouting timeout — on any platform, while a hosted Rust caller that built an
//! `RmwConfig` by hand could state all of it.
//!
//! What is asserted here is the round trip and the refusals, not any particular
//! key: the key set is zenoh-pico's and is checked against its own `config.h`
//! by `just check zpico-config-keys`. The refusals matter as much as the round
//! trip, because the failure this whole work item is about is not "the
//! configuration was rejected" — it is "the configuration vanished and the
//! session reported success".

use core::ffi::{CStr, c_char, c_void};

use nros_rmw::TransportError;
use nros_rmw_cffi::{
    CffiSession, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode, NrosRmwPublisher,
    NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSessionOptions,
    NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

/// What the backend saw, recorded so the test can assert on it after the call.
///
/// One shared slot, so `open_with` below holds `TEST_LOCK` across both the open
/// and the readback. nextest runs the tests in this file as threads of one
/// process; without the lock, two opens interleave and the readback belongs to
/// whichever ran last — a flake that would look like the seam dropping
/// properties, which is precisely the bug under test.
static SEEN: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

unsafe extern "C" fn open(
    _locator: *const c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const c_char,
    options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    let mut seen = Vec::new();
    if !options.is_null() {
        let opts = unsafe { &*options };
        for i in 0..opts.property_count {
            let e = unsafe { &*opts.properties.add(i) };
            seen.push((
                unsafe { CStr::from_ptr(e.key) }
                    .to_str()
                    .unwrap()
                    .to_owned(),
                unsafe { CStr::from_ptr(e.value) }
                    .to_str()
                    .unwrap()
                    .to_owned(),
            ));
        }
    }
    *SEEN.lock().unwrap() = seen;
    unsafe { (*out).backend_data = 0x5E55_1000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn noop_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_create_pub(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_destroy_pub(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_publish(
    _: *const NrosRmwPublisher,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_create_sub(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_destroy_sub(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_take(
    _: *const NrosRmwSubscription,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut bool,
) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_has_data(_: *mut NrosRmwSubscription, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_create_srv(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_destroy_srv(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_take_request(
    _: *const NrosRmwService,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut i64,
    _: *mut bool,
) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_has_request(_: *mut NrosRmwService, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_send_response(
    _: *const NrosRmwService,
    _: i64,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_create_client(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_reg_sub_event(
    _: *const NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_reg_pub_event(
    _: *const NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(open),
    destroy_session: Some(noop_close),
    drive_io: Some(noop_drive_io),
    create_publisher: Some(noop_create_pub),
    destroy_publisher: Some(noop_destroy_pub),
    publish: Some(noop_publish),
    create_subscription: Some(noop_create_sub),
    destroy_subscription: Some(noop_destroy_sub),
    take: Some(noop_take),
    has_data: Some(noop_has_data),
    create_service: Some(noop_create_srv),
    destroy_service: Some(noop_destroy_srv),
    take_request: Some(noop_take_request),
    has_request: Some(noop_has_request),
    send_response: Some(noop_send_response),
    create_client: Some(noop_create_client),
    destroy_client: Some(noop_destroy_client),
    subscription_event_init: Some(noop_reg_sub_event),
    publisher_event_init: Some(noop_reg_pub_event),
    ..EMPTY_VTABLE
};

const BACKEND: &str = "session_props";

/// Register once; every test below opens against this name.
fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let ret = unsafe { nros_rmw_cffi_register_named(c"session_props".as_ptr(), &VTABLE) };
        assert_eq!(
            ret, NROS_RMW_RET_OK,
            "registering the stub backend is a PRECONDITION of every assertion \
             in this file; a failure here means nothing below was tested"
        );
    });
}

/// Open against the stub backend and return both the result and exactly what
/// that open showed the backend, with the two taken under one lock.
fn open_with(
    props: &[(&str, &str)],
) -> (Result<CffiSession, TransportError>, Vec<(String, String)>) {
    register();
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SEEN.lock().unwrap().clear();
    let result = CffiSession::open_named_with_properties(
        BACKEND,
        "tcp/127.0.0.1:7447",
        0,
        0,
        "props",
        props,
    );
    let seen = SEEN.lock().unwrap().clone();
    (result, seen)
}

#[test]
fn properties_reach_the_backend_in_order() {
    let props = [
        ("listen", "tcp/0.0.0.0:7448"),
        ("multicast_scouting", "false"),
        ("tls_root_ca_certificate", "/etc/nros/ca.pem"),
    ];
    let (session, seen) = open_with(&props);
    session.expect("open with properties");
    assert_eq!(
        seen,
        props
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<Vec<_>>(),
        "the backend must see exactly the properties the caller stated, in order"
    );
}

#[test]
fn no_properties_still_means_a_null_options_pointer() {
    let (session, seen) = open_with(&[]);
    session.expect("open with no properties");
    assert!(
        seen.is_empty(),
        "an empty property list must not fabricate entries"
    );
}

#[test]
fn too_many_properties_is_refused_not_truncated() {
    // One past the ABI bound. The old seam had no bound at all because it
    // carried nothing; the zenoh shim's own `.min()` truncation is the shape
    // being ruled out here.
    let owned: Vec<(String, String)> = (0..=nros_rmw_cffi::RMW_SESSION_MAX_PROPERTIES)
        .map(|i| (format!("listen{i}"), format!("v{i}")))
        .collect();
    let props: Vec<(&str, &str)> = owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (result, seen) = open_with(&props);
    assert_eq!(
        result.err(),
        Some(TransportError::InvalidArgument),
        "{} properties must be refused, not silently cut to {}",
        props.len(),
        nros_rmw_cffi::RMW_SESSION_MAX_PROPERTIES
    );
    assert!(
        seen.is_empty(),
        "a refused configuration must not reach the backend at all"
    );
}

#[test]
fn an_empty_key_or_value_is_refused() {
    assert_eq!(
        open_with(&[("", "x")]).0.err(),
        Some(TransportError::InvalidArgument),
        "an empty key is not a key"
    );
    assert_eq!(
        open_with(&[("listen", "")]).0.err(),
        Some(TransportError::InvalidArgument),
        "an empty value is indistinguishable from an unset one on the far side"
    );
}

#[test]
fn an_over_long_value_is_refused_not_clipped() {
    // A TLS certificate path clipped at the buffer edge names a different file
    // — one that probably does not exist, and whose absence is reported by the
    // TLS layer as something unrelated to the configuration that caused it.
    let long = "/".repeat(4096);
    assert_eq!(
        open_with(&[("tls_root_ca_certificate", long.as_str())])
            .0
            .err(),
        Some(TransportError::InvalidArgument),
        "an over-long value must be refused rather than truncated"
    );
}
