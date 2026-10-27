//! The `get_implementation_identifier` slot answers PER SESSION, and a NULL
//! slot answers the name the vtable was REGISTERED under.
//!
//! The slot had no reader until the per-node `rmw_implementation_identifier`
//! accessors (Rust / C / C++) were wired to it, and its header doc had once
//! promised the registry-name fallback with no code behind it (corrected
//! phase-393 W2). These assertions are about WHERE each answer comes from: the
//! backend's own string when it states one, the registry when it does not, and
//! two sessions in one image disagreeing when their backends do.

use core::{
    ffi::{c_char, c_void},
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{RmwConfig, Session, SessionMode};
use nros_rmw_cffi::{
    CffiRmw, CffiSession, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

static OPEN_CALLS: AtomicUsize = AtomicUsize::new(0);

// ---- the answers under test --------------------------------------------

/// A backend that states its own name — and deliberately NOT the name it is
/// registered under in the test below, so the two sources are told apart.
unsafe extern "C" fn stated_name() -> *const c_char {
    c"ii_backend_own_name".as_ptr()
}

unsafe extern "C" fn other_stated_name() -> *const c_char {
    c"ii_other_backend".as_ptr()
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
/// overrides. `nros_rmw_cffi_register_named` REFUSES an all-NULL vtable (issue 0349),
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

static STATED_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_implementation_identifier: Some(stated_name),
    ..BASE
};

static OTHER_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_implementation_identifier: Some(other_stated_name),
    ..BASE
};

/// A backend that states no name — a foreign vtable that predates the reader.
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
fn a_stated_name_wins_over_the_registered_name() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ii_registered_as".as_ptr(), &STATED_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let session =
        CffiRmw::open_with_rmw("ii_registered_as", &config("ii_stated_node")).expect("open stated");

    // The backend's own string is the one its gids carry, so it is the answer
    // even when the image registered the table under another name.
    assert_eq!(
        session.implementation_identifier(),
        Some("ii_backend_own_name")
    );
    // Down the TRAIT too: an inherent method shadows a trait method of the
    // same name, so the call above proves nothing about the user-reachable one.
    assert_eq!(
        <CffiSession as Session>::implementation_identifier(&session),
        Some("ii_backend_own_name"),
    );
}

#[test]
fn a_null_slot_answers_the_registered_name() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ii_silent".as_ptr(), &SILENT_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let session =
        CffiRmw::open_with_rmw("ii_silent", &config("ii_silent_node")).expect("open silent");

    // NOT `None`, and not a guess: the name the registry holds for this table.
    assert_eq!(session.implementation_identifier(), Some("ii_silent"));
    assert_eq!(
        <CffiSession as Session>::implementation_identifier(&session),
        Some("ii_silent"),
        "the trait method must fall back exactly as the inherent one does",
    );
    // Static storage: two calls, one address.
    let first = session.implementation_identifier_cstr();
    assert!(!first.is_null());
    assert_eq!(first, session.implementation_identifier_cstr());
}

#[test]
fn two_sessions_report_their_own_backends_names() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ii_a".as_ptr(), &STATED_VTABLE) },
        NROS_RMW_RET_OK,
    );
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"ii_b".as_ptr(), &OTHER_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let a = CffiRmw::open_with_rmw("ii_a", &config("ii_a_node")).expect("open a");
    let b = CffiRmw::open_with_rmw("ii_b", &config("ii_b_node")).expect("open b");

    // One image, two backends, two answers — what no image-wide constant holds.
    assert_eq!(a.implementation_identifier(), Some("ii_backend_own_name"));
    assert_eq!(b.implementation_identifier(), Some("ii_other_backend"));
}
