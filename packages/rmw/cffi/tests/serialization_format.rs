//! RFC-0088 D4 / phase-421 W2 — the `get_serialization_format` slot answers
//! PER SESSION, and two sessions in one image can disagree.
//!
//! That disagreement is the whole reason the slot stopped being reserved. Every
//! other route to the format is a compile-time constant —
//! `nros_node::IMAGE_SERIALIZATION_FORMAT`, the generated
//! `NROS_SERIALIZATION_FORMAT` macro, the `Session::SERIALIZATION_FORMAT`
//! default — and a compile-time constant has exactly one value per image, while
//! `nros_rmw_cffi_register_named` admits several backends at once. A bridge
//! image linking a CDR backend to uORB therefore has two right answers and no
//! constant that can hold both.
//!
//! So the assertions here are about WHERE the answer comes from, not about what
//! it says: two sessions, opened in one process against two registered vtables,
//! must report the two vtables' own formats. A global would make them agree,
//! which is exactly the failure this test is shaped to catch.

use core::{
    ffi::{c_char, c_void},
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{RmwConfig, SessionMode};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode, NrosRmwPublisher,
    NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSessionOptions,
    NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

static OPEN_CALLS: AtomicUsize = AtomicUsize::new(0);

// ---- the three answers under test --------------------------------------

/// A DDS-derived / zenoh-shaped backend: OMG CDR, like every in-tree backend
/// except uORB.
unsafe extern "C" fn cdr_format() -> *const c_char {
    c"cdr".as_ptr()
}

/// uORB: the payload IS the PX4 struct, so there is no encoding step at all
/// (RFC-0011). The one in-tree backend whose answer differs.
unsafe extern "C" fn uorb_format() -> *const c_char {
    c"uorb".as_ptr()
}

// ---- minimal backend plumbing ------------------------------------------

unsafe extern "C" fn open(
    _locator: *const c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const c_char,
    _options: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    OPEN_CALLS.fetch_add(1, Ordering::SeqCst);
    unsafe { (*out).backend_data = 0x5F00_0001usize as *mut c_void };
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

/// Every required slot filled; the format slot is what each backend below
/// overrides. `nros_rmw_cffi_register` REFUSES an all-NULL vtable (issue 0349),
/// so the base has to be real even though this test never publishes.
const BASE: NrosRmwVtable = NrosRmwVtable {
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

static CDR_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_serialization_format: Some(cdr_format),
    ..BASE
};

static UORB_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_serialization_format: Some(uorb_format),
    ..BASE
};

/// A backend that never declares a format — a foreign or pre-phase-421 vtable.
static SILENT_VTABLE: NrosRmwVtable = BASE;

fn config(node_name: &'static str) -> RmwConfig<'static> {
    RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name,
        namespace: "",
        properties: &[],
    }
}

#[test]
fn two_sessions_report_their_own_backends_formats() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"sf_cdr".as_ptr(), &CDR_VTABLE) },
        NROS_RMW_RET_OK,
    );
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"sf_uorb".as_ptr(), &UORB_VTABLE) },
        NROS_RMW_RET_OK,
    );

    let session_cdr = CffiRmw::open_with_rmw("sf_cdr", &config("sf_cdr_node")).expect("open cdr");
    let session_uorb =
        CffiRmw::open_with_rmw("sf_uorb", &config("sf_uorb_node")).expect("open uorb");

    assert_eq!(session_cdr.serialization_format(), Some("cdr"));
    assert_eq!(session_uorb.serialization_format(), Some("uorb"));

    // The point of the whole slot: one image, two live sessions, two answers.
    // A global — or the `Session::SERIALIZATION_FORMAT` const, which is `"cdr"`
    // for `CffiSession` whatever vtable it holds — would make these equal.
    assert_ne!(
        session_cdr.serialization_format(),
        session_uorb.serialization_format(),
        "both sessions reported the same format, so the answer is not coming \
         from the session's own vtable (RFC-0088 D4)",
    );
}

#[test]
fn a_backend_that_declares_nothing_answers_none_not_cdr() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"sf_silent".as_ptr(), &SILENT_VTABLE) },
        NROS_RMW_RET_OK,
    );

    let session =
        CffiRmw::open_with_rmw("sf_silent", &config("sf_silent_node")).expect("open silent");

    // NOT `Some("cdr")`. A backend that has not said what it speaks has not
    // said CDR, and inventing an answer on its behalf is the fallback the
    // sibling `get_implementation_identifier` slot spent two phases promising
    // in its doc comment with no code behind it (corrected phase-393 W2).
    assert_eq!(session.serialization_format(), None);
    assert!(session.serialization_format_cstr().is_null());
}

#[test]
fn the_format_pointer_is_static_storage() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"sf_static".as_ptr(), &UORB_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let session =
        CffiRmw::open_with_rmw("sf_static", &config("sf_static_node")).expect("open static");

    // The slot's contract is a `const char *` that outlives the call. Two calls
    // returning the same address is the cheap, checkable half of that; a body
    // formatting into a per-call buffer would fail here rather than in whatever
    // reads the string later.
    let first = session.serialization_format_cstr();
    let second = session.serialization_format_cstr();
    assert!(!first.is_null());
    assert_eq!(first, second);
}
