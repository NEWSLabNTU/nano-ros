//! Phase 124.A.3 — arena-fallback loan path test.
//!
//! Verifies that a backend with `pub_loan == NULL` still satisfies
//! `SlotLending::try_lend_slot`: the runtime allocates a staging
//! buffer, hands the caller a writable slot, and on `commit_slot`
//! emits a single `publish_raw` of the cursor-truncated contents.

#![cfg(all(feature = "lending", feature = "alloc"))]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{QosSettings, RmwConfig, Session as _, SessionMode, SlotLending, TopicInfo};
use nros_rmw_cffi::{
    CffiRmw, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwEventCallback, NrosRmwEventKind,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwServiceClient, NrosRmwServiceServer,
    NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable, nros_rmw_cffi_register_named,
};

static PUBLISH_CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST_PUBLISH_LEN: AtomicUsize = AtomicUsize::new(0);
static LAST_PUBLISH_BYTES: [AtomicUsize; 16] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

unsafe extern "C" fn open(
    _: *const u8,
    _: u8,
    _: u32,
    _: *const u8,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xF0F0_F0F0usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn create_publisher(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0xCAFEusize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_publisher(_: *mut NrosRmwPublisher) {}
unsafe extern "C" fn publish_raw(
    _: *mut NrosRmwPublisher,
    data: *const u8,
    len: usize,
) -> NrosRmwRet {
    PUBLISH_CALLS.fetch_add(1, Ordering::SeqCst);
    LAST_PUBLISH_LEN.store(len, Ordering::SeqCst);
    let slice = unsafe { core::slice::from_raw_parts(data, len) };
    for (i, b) in slice.iter().enumerate().take(LAST_PUBLISH_BYTES.len()) {
        LAST_PUBLISH_BYTES[i].store(*b as usize, Ordering::SeqCst);
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csub(
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
unsafe extern "C" fn noop_dsub(_: *mut NrosRmwSubscriber) {}
unsafe extern "C" fn noop_recv(_: *mut NrosRmwSubscriber, _: *mut u8, _: usize) -> i32 {
    -1
}
unsafe extern "C" fn noop_hasd(_: *mut NrosRmwSubscriber) -> i32 {
    0
}
unsafe extern "C" fn noop_csrv(
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
unsafe extern "C" fn noop_dsrv(_: *mut NrosRmwServiceServer) {}
unsafe extern "C" fn noop_recvreq(
    _: *mut NrosRmwServiceServer,
    _: *mut u8,
    _: usize,
    _: *mut i64,
) -> i32 {
    -1
}
unsafe extern "C" fn noop_hasreq(_: *mut NrosRmwServiceServer) -> i32 {
    0
}
unsafe extern "C" fn noop_reply(
    _: *mut NrosRmwServiceServer,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_ccli(
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
unsafe extern "C" fn noop_dcli(_: *mut NrosRmwServiceClient) {}
unsafe extern "C" fn noop_call(
    _: *mut NrosRmwServiceClient,
    _: *const u8,
    _: usize,
    _: *mut u8,
    _: usize,
) -> i32 {
    -1
}
unsafe extern "C" fn noop_regsubev(
    _: *mut NrosRmwSubscriber,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_regpubev(
    _: *mut NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_alv(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
    open,
    close,
    drive_io,
    create_publisher,
    destroy_publisher,
    publish_raw,
    create_subscriber: noop_csub,
    destroy_subscriber: noop_dsub,
    try_recv_raw: noop_recv,
    has_data: noop_hasd,
    create_service_server: noop_csrv,
    destroy_service_server: noop_dsrv,
    try_recv_request: noop_recvreq,
    has_request: noop_hasreq,
    send_reply: noop_reply,
    create_service_client: noop_ccli,
    destroy_service_client: noop_dcli,
    call_raw: noop_call,
    send_request_raw: None,
    try_recv_reply_raw: None,
    register_subscriber_event: noop_regsubev,
    register_publisher_event: noop_regpubev,
    assert_publisher_liveliness: noop_alv,
    next_deadline_ms: None,
    set_wake_callback: None,
    // Phase 124.A.3 — NULL pub_loan: runtime falls back to arena.
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
};

#[test]
fn arena_fallback_commit_emits_publish_raw() {
    let ret = unsafe { nros_rmw_cffi_register_named(c"lf_arena".as_ptr(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let cfg = RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name: "lf_arena_node",
        namespace: "",
        properties: &[],
    };
    let mut session = CffiRmw::open_with_rmw("lf_arena", &cfg).expect("open");

    let topic = TopicInfo::new("/lf", "std_msgs/msg/Int32", "RIHS01_lf");
    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");

    let initial = PUBLISH_CALLS.load(Ordering::SeqCst);
    let mut slot = publisher
        .try_lend_slot(8)
        .expect("try_lend_slot")
        .expect("backend NULL → fallback should yield Some");
    let buf = slot.as_mut();
    assert_eq!(buf.len(), 8);
    buf[..5].copy_from_slice(b"HELLO");
    slot.set_len(5);
    publisher.commit_slot(slot).expect("commit_slot");

    assert_eq!(
        PUBLISH_CALLS.load(Ordering::SeqCst),
        initial + 1,
        "commit must emit exactly one publish_raw"
    );
    assert_eq!(LAST_PUBLISH_LEN.load(Ordering::SeqCst), 5);
    let expected = b"HELLO";
    for (i, b) in expected.iter().enumerate() {
        assert_eq!(
            LAST_PUBLISH_BYTES[i].load(Ordering::SeqCst) as u8,
            *b,
            "byte {i} mismatch",
        );
    }
}

#[test]
fn arena_fallback_drop_discards() {
    // nextest forks per test, so each test must re-register the
    // backend into its own fresh registry.
    let ret = unsafe { nros_rmw_cffi_register_named(c"lf_arena".as_ptr(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK);

    let cfg = RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7448",
        domain_id: 0,
        node_name: "lf_arena_node2",
        namespace: "",
        properties: &[],
    };
    let mut session = CffiRmw::open_with_rmw("lf_arena", &cfg).expect("open");
    let topic = TopicInfo::new("/lf2", "std_msgs/msg/Int32", "RIHS01_lf");
    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");

    let pre = PUBLISH_CALLS.load(Ordering::SeqCst);
    {
        let mut slot = publisher
            .try_lend_slot(8)
            .expect("try_lend_slot")
            .expect("fallback Some");
        slot.as_mut()[0] = 0xAA;
        // Drop slot without commit → arena release, no publish_raw.
    }
    assert_eq!(
        PUBLISH_CALLS.load(Ordering::SeqCst),
        pre,
        "dropped (uncommitted) slot must NOT emit publish_raw",
    );
}
