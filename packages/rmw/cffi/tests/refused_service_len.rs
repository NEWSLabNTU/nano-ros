//! Issue 1632 — a too-small `take_request` / `take_response` reaches the caller
//! as `refused_request_len` / `refused_response_len`, beside the
//! `BufferTooSmall` it always produced.
//!
//! The same three readings issue 1612's `take_sequence.rs` cell takes for the
//! subscription `take`: the size when the backend wrote one, UNKNOWN when it
//! did not (a backend written before the rule), and UNKNOWN when the number it
//! wrote would have FIT (a breach is not a size). The stub also asserts the
//! caller pre-set `len` to `NROS_RMW_TAKE_LEN_UNKNOWN` — without that a backend
//! that never writes `len` reports whatever was on the stack.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU8, Ordering},
};

use nros_rmw::{ClientTrait, ServiceInfo, ServiceTrait, Session, SessionMode, TransportError};
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_QOS_POLICY_CORE, NROS_RMW_RET_BUFFER_TOO_SMALL, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwService, NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    generated, nros_rmw_cffi_register_named,
};

const REFUSED_LEN: usize = 700;
const WRITES_SIZE: u8 = 0;
const LEAVES_LEN: u8 = 1;
const WRITES_FITTING_LEN: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(WRITES_SIZE);

fn refuse(span: *mut generated::rmw_mut_byte_span_t) -> NrosRmwRet {
    unsafe {
        assert_eq!(
            (*span).len,
            generated::NROS_RMW_TAKE_LEN_UNKNOWN as usize,
            "the caller must pre-set `len` to UNKNOWN"
        );
        match MODE.load(Ordering::SeqCst) {
            WRITES_SIZE => (*span).len = REFUSED_LEN,
            WRITES_FITTING_LEN => (*span).len = 1,
            _ => {}
        }
    }
    NROS_RMW_RET_BUFFER_TOO_SMALL
}

unsafe extern "C" fn open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    _: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn create_publisher(
    _: *const NrosRmwNode,
    _: *const generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn publish(
    _: *const NrosRmwPublisher,
    _: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn create_subscription(
    _: *const NrosRmwNode,
    _: *const generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn destroy_subscription(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn take(
    _: *const NrosRmwSubscription,
    _: *mut generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn has_data(_: *mut NrosRmwSubscription, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn create_service(
    _: *const NrosRmwNode,
    _: *const generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwService,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_service(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn take_request(
    _: *const NrosRmwService,
    span: *mut generated::rmw_mut_byte_span_t,
    _: *mut i64,
    _: *mut bool,
) -> NrosRmwRet {
    refuse(span)
}
unsafe extern "C" fn has_request(_: *mut NrosRmwService, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = true };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn send_response(
    _: *const NrosRmwService,
    _: i64,
    _: generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn create_client(
    _: *const NrosRmwNode,
    _: *const generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn take_response(
    _: *const NrosRmwClient,
    span: *mut generated::rmw_mut_byte_span_t,
    _: *mut i64,
    _: *mut bool,
) -> NrosRmwRet {
    refuse(span)
}
unsafe extern "C" fn supported_qos(_: *const NrosRmwSession, mask: *mut u32) -> NrosRmwRet {
    unsafe { *mask = NROS_RMW_QOS_POLICY_CORE as u32 };
    NROS_RMW_RET_OK
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(open),
    destroy_session: Some(close),
    drive_io: Some(drive_io),
    create_publisher: Some(create_publisher),
    destroy_publisher: Some(destroy_publisher),
    publish: Some(publish),
    create_subscription: Some(create_subscription),
    destroy_subscription: Some(destroy_subscription),
    take: Some(take),
    has_data: Some(has_data),
    create_service: Some(create_service),
    destroy_service: Some(destroy_service),
    take_request: Some(take_request),
    has_request: Some(has_request),
    send_response: Some(send_response),
    create_client: Some(create_client),
    destroy_client: Some(destroy_client),
    take_response: Some(take_response),
    supported_qos_policies: Some(supported_qos),
    ..EMPTY_VTABLE
};

fn session() -> nros_rmw_cffi::CffiSession {
    let ret = unsafe { nros_rmw_cffi_register_named(c"refused_srv".as_ptr(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK);
    nros_rmw_cffi::CffiSession::open_named(
        "refused_srv",
        "tcp/127.0.0.1:7447",
        SessionMode::Client as u8,
        0,
        "stub_node",
        "",
    )
    .expect("open_named")
}

const CASES: [(u8, Option<usize>, &str); 3] = [
    (
        WRITES_SIZE,
        Some(REFUSED_LEN),
        "the backend said the payload needed 700 bytes; a drop log can only \
         print what reaches here",
    ),
    (
        LEAVES_LEN,
        None,
        "a backend that never writes `len` must read as unknown",
    ),
    (
        WRITES_FITTING_LEN,
        None,
        "a `len` that would have fit is a breach, not a size",
    ),
];

/// Both halves in ONE test: `MODE` is a file global, and two tests driving it
/// concurrently would read each other's mode.
#[test]
fn refused_service_takes_report_the_size_the_payload_needed() {
    let mut s = session();
    let info = ServiceInfo::new("/refused", "example/Srv", "RIHS01_srv");
    let qos = nros_rmw::QoSProfile::services_default();

    let mut server = s.create_service(&info, qos).expect("create_service");
    assert_eq!(server.refused_request_len(), None, "nothing refused yet");
    let mut buf = [0u8; 16];
    for (mode, want, why) in CASES {
        MODE.store(mode, Ordering::SeqCst);
        assert!(matches!(
            server.take_request(&mut buf),
            Err(TransportError::BufferTooSmall)
        ));
        assert_eq!(server.refused_request_len(), want, "request: {why}");
    }

    let mut client = s.create_client(&info, qos).expect("create_client");
    assert_eq!(client.refused_response_len(), None, "nothing refused yet");
    for (mode, want, why) in CASES {
        MODE.store(mode, Ordering::SeqCst);
        assert_eq!(
            client.take_response_raw(&mut buf),
            Err(TransportError::BufferTooSmall)
        );
        assert_eq!(client.refused_response_len(), want, "response: {why}");
    }
    core::mem::forget(server);
    core::mem::forget(client);
    core::mem::forget(s);
}
