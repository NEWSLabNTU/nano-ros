//! Phase 104.C.7 — two simultaneous sessions across two registered
//! backends. Exercises the named-registry session-routing path:
//! `CffiRmw::open_with_rmw("name", ...)` must return a `CffiSession`
//! whose subsequent calls (drive_io, publish, etc.) dispatch to the
//! correct backend's vtable, not the first-registered one.

use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{RmwConfig, Session as _, SessionMode, TopicInfo};
use nros_rmw_cffi::{
    CffiRmw, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwEventCallback,
    NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

// ---- Backend A ----------------------------------------------------------

static A_OPEN_CALLS: AtomicUsize = AtomicUsize::new(0);
static A_DRIVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static A_PUBLISH_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn a_open(
    _locator: *const core::ffi::c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const core::ffi::c_char,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    A_OPEN_CALLS.fetch_add(1, Ordering::SeqCst);
    // Tag the session so the per-backend fn pointers can spot a
    // routing bug if the runtime ever crossed the wires.
    unsafe { (*out).backend_data = 0xA000_0000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn a_drive_io(session: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    assert_eq!(unsafe { (*session).backend_data } as usize, 0xA000_0000);
    A_DRIVE_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

unsafe extern "C" fn a_create_publisher(
    _session: *mut NrosRmwSession,
    _topic_name: *const core::ffi::c_char,
    _type_name: *const core::ffi::c_char,
    _type_hash: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::nros_rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0xA000_0001usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn a_publish_raw(
    publisher: *mut NrosRmwPublisher,
    _data: *const u8,
    _len: usize,
) -> NrosRmwRet {
    assert_eq!(unsafe { (*publisher).backend_data } as usize, 0xA000_0001);
    A_PUBLISH_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

// ---- Backend B ----------------------------------------------------------

static B_OPEN_CALLS: AtomicUsize = AtomicUsize::new(0);
static B_DRIVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static B_PUBLISH_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn b_open(
    _locator: *const core::ffi::c_char,
    _mode: u8,
    _domain_id: u32,
    _node_name: *const core::ffi::c_char,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    B_OPEN_CALLS.fetch_add(1, Ordering::SeqCst);
    unsafe { (*out).backend_data = 0xB000_0000usize as *mut c_void };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn b_drive_io(session: *mut NrosRmwSession, _timeout_ms: i32) -> NrosRmwRet {
    assert_eq!(unsafe { (*session).backend_data } as usize, 0xB000_0000);
    B_DRIVE_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

unsafe extern "C" fn b_create_publisher(
    _session: *mut NrosRmwSession,
    _topic_name: *const core::ffi::c_char,
    _type_name: *const core::ffi::c_char,
    _type_hash: *const core::ffi::c_char,
    _domain_id: u32,
    _qos: *const NrosRmwQos,
    _options: *const nros_rmw_cffi::nros_rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0xB000_0001usize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn b_publish_raw(
    publisher: *mut NrosRmwPublisher,
    _data: *const u8,
    _len: usize,
) -> NrosRmwRet {
    assert_eq!(unsafe { (*publisher).backend_data } as usize, 0xB000_0001);
    B_PUBLISH_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

// ---- Shared no-op stubs (close + every other slot) ----------------------

unsafe extern "C" fn noop_close(_session: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_destroy_pub(_p: *mut NrosRmwPublisher) {}
unsafe extern "C" fn noop_create_sub(
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
unsafe extern "C" fn noop_destroy_sub(_: *mut NrosRmwSubscription) {}
unsafe extern "C" fn noop_try_recv_raw(_: *mut NrosRmwSubscription, _: *mut u8, _: usize) -> i32 {
    -1
}
unsafe extern "C" fn noop_has_data(_: *mut NrosRmwSubscription) -> i32 {
    0
}
unsafe extern "C" fn noop_create_srv(
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
unsafe extern "C" fn noop_destroy_srv(_: *mut NrosRmwService) {}
unsafe extern "C" fn noop_try_recv_req(
    _: *mut NrosRmwService,
    _: *mut u8,
    _: usize,
    _: *mut i64,
) -> i32 {
    -1
}
unsafe extern "C" fn noop_has_request(_: *mut NrosRmwService) -> i32 {
    0
}
unsafe extern "C" fn noop_send_reply(
    _: *mut NrosRmwService,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_create_client(
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
unsafe extern "C" fn noop_destroy_client(_: *mut NrosRmwClient) {}
unsafe extern "C" fn noop_reg_sub_event(
    _: *mut NrosRmwSubscription,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_reg_pub_event(
    _: *mut NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_assert_liveliness(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

static A_VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(a_open),
    destroy_session: Some(noop_close),
    drive_io: Some(a_drive_io),
    create_publisher: Some(a_create_publisher),
    destroy_publisher: Some(noop_destroy_pub),
    publish_raw: Some(a_publish_raw),
    create_subscription: Some(noop_create_sub),
    destroy_subscription: Some(noop_destroy_sub),
    try_recv_raw: Some(noop_try_recv_raw),
    has_data: Some(noop_has_data),
    create_service: Some(noop_create_srv),
    destroy_service: Some(noop_destroy_srv),
    try_recv_request: Some(noop_try_recv_req),
    has_request: Some(noop_has_request),
    send_reply: Some(noop_send_reply),
    create_client: Some(noop_create_client),
    destroy_client: Some(noop_destroy_client),
    send_request_raw: None,
    try_recv_reply_raw: None,
    register_subscription_event: Some(noop_reg_sub_event),
    register_publisher_event: Some(noop_reg_pub_event),
    assert_publisher_liveliness: Some(noop_assert_liveliness),
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

static B_VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(b_open),
    destroy_session: Some(noop_close),
    drive_io: Some(b_drive_io),
    create_publisher: Some(b_create_publisher),
    destroy_publisher: Some(noop_destroy_pub),
    publish_raw: Some(b_publish_raw),
    create_subscription: Some(noop_create_sub),
    destroy_subscription: Some(noop_destroy_sub),
    try_recv_raw: Some(noop_try_recv_raw),
    has_data: Some(noop_has_data),
    create_service: Some(noop_create_srv),
    destroy_service: Some(noop_destroy_srv),
    try_recv_request: Some(noop_try_recv_req),
    has_request: Some(noop_has_request),
    send_reply: Some(noop_send_reply),
    create_client: Some(noop_create_client),
    destroy_client: Some(noop_destroy_client),
    send_request_raw: None,
    try_recv_reply_raw: None,
    register_subscription_event: Some(noop_reg_sub_event),
    register_publisher_event: Some(noop_reg_pub_event),
    assert_publisher_liveliness: Some(noop_assert_liveliness),
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

#[test]
fn two_sessions_route_to_correct_vtable() {
    // Register both backends under distinct names.
    let ra = unsafe { nros_rmw_cffi_register_named(c"tb_a".as_ptr(), &A_VTABLE) };
    let rb = unsafe { nros_rmw_cffi_register_named(c"tb_b".as_ptr(), &B_VTABLE) };
    assert_eq!(ra, NROS_RMW_RET_OK);
    assert_eq!(rb, NROS_RMW_RET_OK);

    let cfg_a = RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name: "tb_a_node",
        namespace: "",
        properties: &[],
    };
    let cfg_b = RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7448",
        domain_id: 0,
        node_name: "tb_b_node",
        namespace: "",
        properties: &[],
    };

    let mut session_a = CffiRmw::open_with_rmw("tb_a", &cfg_a).expect("open tb_a");
    let mut session_b = CffiRmw::open_with_rmw("tb_b", &cfg_b).expect("open tb_b");

    assert_eq!(A_OPEN_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(B_OPEN_CALLS.load(Ordering::SeqCst), 1);

    // Drive each session — counter for the wrong backend must stay flat.
    session_a.drive_io(0).expect("drive_io a");
    assert_eq!(A_DRIVE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(B_DRIVE_CALLS.load(Ordering::SeqCst), 0);

    session_b.drive_io(0).expect("drive_io b");
    assert_eq!(A_DRIVE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(B_DRIVE_CALLS.load(Ordering::SeqCst), 1);

    // Publish: create_publisher tags the publisher; publish_raw asserts
    // the tag matches the publisher's backend. Routing bug would trip
    // the in-stub assert.
    let topic_a = TopicInfo::new("/tb_a", "std_msgs/msg/Int32", "RIHS01_a");
    let topic_b = TopicInfo::new("/tb_b", "std_msgs/msg/Int32", "RIHS01_b");
    let qos = nros_rmw::QosSettings::default();
    let pub_a = session_a.create_publisher(&topic_a, qos).expect("pub a");
    let pub_b = session_b.create_publisher(&topic_b, qos).expect("pub b");
    use nros_rmw::Publisher as _;
    pub_a.publish_raw(&[1u8]).expect("publish a");
    pub_b.publish_raw(&[2u8, 3]).expect("publish b");
    pub_a.publish_raw(&[4u8]).expect("publish a #2");

    assert_eq!(A_PUBLISH_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(B_PUBLISH_CALLS.load(Ordering::SeqCst), 1);

    // Quiet the lints — the loops above keep the publishers alive
    // through the asserts; explicit drops document intent.
    drop(pub_a);
    drop(pub_b);
    let _ = ptr::null::<()>(); // suppress unused-use
}
