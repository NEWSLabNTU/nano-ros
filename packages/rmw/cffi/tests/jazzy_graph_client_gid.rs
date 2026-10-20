//! Issue 1978 — the Jazzy contract's four ADOPT slots, through the C ABI.
//!
//! `count_clients` / `count_services` / `get_gid_for_client` follow the
//! NULL-slot discipline of their live siblings (`count_publishers`,
//! `get_gid_for_publisher`): a NULL slot is `Unsupported`, never `Ok(0)` or an
//! all-zero gid, and a filled slot is forwarded to the RIGHT slot with the
//! caller's name. `event_type_is_supported` is read through ONE helper,
//! `backend_event_type_is_supported`, which the per-entity `supports_event`
//! also asks — so the two can never disagree.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::sync::Mutex;

use nros_rmw::{
    ClientTrait as _, EventKind, Publisher as _, QoSProfile, RmwConfig, ServiceInfo, Session as _,
    SessionMode, TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwGid, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, backend_event_type_is_supported,
    nros_rmw_cffi_register_named,
};

/// Distinct counts, so a forward wired to the other slot reads the wrong one.
const CLIENTS: usize = 5;
const SERVICES: usize = 9;

/// Not all-zero, not uniform — see `publisher_gid.rs` for why.
const CLIENT_GID: [u8; 24] = [
    0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The name the last count slot was handed.
static LAST_NAME: Mutex<String> = Mutex::new(String::new());
static EVENT_QUERIES: AtomicUsize = AtomicUsize::new(0);

fn record_name(name: *const core::ffi::c_char) {
    let s = unsafe { core::ffi::CStr::from_ptr(name) }
        .to_str()
        .unwrap()
        .to_owned();
    *LAST_NAME.lock().unwrap() = s;
}

unsafe extern "C" fn filled_count_clients(
    _: *const NrosRmwSession,
    name: *const core::ffi::c_char,
    count: *mut usize,
) -> NrosRmwRet {
    record_name(name);
    unsafe { *count = CLIENTS };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn filled_count_services(
    _: *const NrosRmwSession,
    name: *const core::ffi::c_char,
    count: *mut usize,
) -> NrosRmwRet {
    record_name(name);
    unsafe { *count = SERVICES };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn filled_get_gid_for_client(
    _: *const NrosRmwClient,
    gid: *mut NrosRmwGid,
) -> NrosRmwRet {
    unsafe { (*gid).data = CLIENT_GID };
    NROS_RMW_RET_OK
}

/// A backend that carries ONLY `OfferedDeadlineMissed` on the publisher side
/// and `MessageLost` on the subscription side — narrower than its event
/// slots, which is the case the per-kind query exists for.
unsafe extern "C" fn narrow_event_type_is_supported(kind: NrosRmwEventKind) -> bool {
    use nros_rmw_cffi::rmw_event_type_t as k;
    EVENT_QUERIES.fetch_add(1, Ordering::SeqCst);
    kind == k::NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED || kind == k::NROS_RMW_EVENT_MESSAGE_LOST
}

// ---- minimal backend plumbing -------------------------------------------

unsafe extern "C" fn open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    _: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0x6110_0001usize as *mut c_void };
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
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x6110_0002usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn publish_raw(
    _: *const NrosRmwPublisher,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csub(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_subscription_options_t,
    _: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_dsub(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recv(
    _: *const NrosRmwSubscription,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut bool,
) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_hasd(_: *mut NrosRmwSubscription, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csrv(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_dsrv(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recvreq(
    _: *const NrosRmwService,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut i64,
    _: *mut bool,
) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_hasreq(_: *mut NrosRmwService, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_reply(
    _: *const NrosRmwService,
    _: i64,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_ccli(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0x6110_0003usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_dcli(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_regsubev(
    _: *const NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_regpubev(
    _: *const NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

/// Every REQUIRED slot filled, both event paths present (so the NULL
/// `event_type_is_supported` fallback has something to fall back TO), and the
/// four issue-1978 slots NULL — the state XRCE and uORB ship for the counts.
const BASE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(open),
    destroy_session: Some(close),
    drive_io: Some(drive_io),
    create_publisher: Some(create_publisher),
    destroy_publisher: Some(destroy_publisher),
    publish: Some(publish_raw),
    create_subscription: Some(noop_csub),
    destroy_subscription: Some(noop_dsub),
    take: Some(noop_recv),
    has_data: Some(noop_hasd),
    create_service: Some(noop_csrv),
    destroy_service: Some(noop_dsrv),
    take_request: Some(noop_recvreq),
    has_request: Some(noop_hasreq),
    send_response: Some(noop_reply),
    create_client: Some(noop_ccli),
    destroy_client: Some(noop_dcli),
    subscription_event_init: Some(noop_regsubev),
    publisher_event_init: Some(noop_regpubev),
    ..EMPTY_VTABLE
};

static SILENT_VTABLE: NrosRmwVtable = BASE;

static FILLED_VTABLE: NrosRmwVtable = NrosRmwVtable {
    count_clients: Some(filled_count_clients),
    count_services: Some(filled_count_services),
    get_gid_for_client: Some(filled_get_gid_for_client),
    event_type_is_supported: Some(narrow_event_type_is_supported),
    ..BASE
};

const ALL_KINDS: [EventKind; 5] = [
    EventKind::LivelinessChanged,
    EventKind::RequestedDeadlineMissed,
    EventKind::MessageLost,
    EventKind::LivelinessLost,
    EventKind::OfferedDeadlineMissed,
];

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

// Two sessions in this binary: the node-slot table is process-wide and never
// released (see `publisher_gid.rs`'s note on test count).

#[test]
fn null_jazzy_slots_answer_unsupported_never_zero() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"jazzy_silent".as_ptr(), &SILENT_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("jazzy_silent", &config("jazzy_silent_node")).expect("open");

    // NOT `Ok(0)`: zero claims there are none.
    assert_eq!(
        session.count_clients("/add_two_ints"),
        Err(TransportError::Unsupported)
    );
    assert_eq!(
        session.count_services("/add_two_ints"),
        Err(TransportError::Unsupported)
    );

    let client = session
        .create_client(
            &ServiceInfo::new("/add_two_ints", "example/Stub", "RIHS01_stub"),
            QoSProfile::services_default(),
        )
        .expect("create_client");
    // NOT `Ok([0; 24])` — the staged gid is zeroed before the call.
    assert_eq!(client.get_gid(), Err(TransportError::Unsupported));

    // NULL `event_type_is_supported` falls back to slot granularity: both
    // event paths exist, so every kind is supported on its own side.
    for kind in ALL_KINDS {
        assert!(
            backend_event_type_is_supported(&SILENT_VTABLE, kind),
            "{kind:?}"
        );
    }
    let publisher = session
        .create_publisher(
            &TopicInfo::new("/jazzy", "std_msgs/msg/Int32", "RIHS01_jazzy"),
            QoSProfile::default(),
        )
        .expect("create publisher");
    assert!(publisher.supports_event(EventKind::LivelinessLost));
    assert!(
        !publisher.supports_event(EventKind::MessageLost),
        "a subscription-side kind is never a publisher's"
    );

    // No event path at all (XRCE, uORB): `false` for every kind.
    static NO_EVENTS: NrosRmwVtable = NrosRmwVtable {
        subscription_event_init: None,
        publisher_event_init: None,
        ..BASE
    };
    for kind in ALL_KINDS {
        assert!(
            !backend_event_type_is_supported(&NO_EVENTS, kind),
            "{kind:?}"
        );
    }
}

#[test]
fn filled_jazzy_slots_forward_to_the_right_slot() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"jazzy_filled".as_ptr(), &FILLED_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("jazzy_filled", &config("jazzy_filled_node")).expect("open");

    assert_eq!(session.count_clients("/add_two_ints"), Ok(CLIENTS));
    assert_eq!(LAST_NAME.lock().unwrap().as_str(), "/add_two_ints");
    assert_eq!(session.count_services("/other_service"), Ok(SERVICES));
    assert_eq!(
        LAST_NAME.lock().unwrap().as_str(),
        "/other_service",
        "the caller's service name must reach the slot unchanged"
    );

    let client = session
        .create_client(
            &ServiceInfo::new("/add_two_ints", "example/Stub", "RIHS01_stub"),
            QoSProfile::services_default(),
        )
        .expect("create_client");
    assert_eq!(client.get_gid(), Ok(CLIENT_GID));

    // The backend's per-kind answer narrows the slot-granularity one, and the
    // per-entity `supports_event` reads the SAME helper.
    let before = EVENT_QUERIES.load(Ordering::SeqCst);
    for kind in ALL_KINDS {
        let want = matches!(
            kind,
            EventKind::OfferedDeadlineMissed | EventKind::MessageLost
        );
        assert_eq!(
            backend_event_type_is_supported(&FILLED_VTABLE, kind),
            want,
            "{kind:?}"
        );
    }
    assert!(
        EVENT_QUERIES.load(Ordering::SeqCst) > before,
        "the slot must be asked"
    );
    let publisher = session
        .create_publisher(
            &TopicInfo::new("/jazzy", "std_msgs/msg/Int32", "RIHS01_jazzy"),
            QoSProfile::default(),
        )
        .expect("create publisher");
    assert!(publisher.supports_event(EventKind::OfferedDeadlineMissed));
    assert!(
        !publisher.supports_event(EventKind::LivelinessLost),
        "a kind the backend denies must not be claimed per entity"
    );
}
