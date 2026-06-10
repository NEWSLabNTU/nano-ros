//! Phase 124.A.8 — native zero-copy loan path test.
//!
//! Counterpart to `loan_fallback.rs`. Verifies that a backend exposing
//! the `pub_loan` / `pub_commit` / `pub_discard` vtable slots routes
//! `SlotLending::try_lend_slot` through them instead of falling back
//! to the arena. Together the two test files cover the "native" and
//! "fallback" halves of the loan path.

#![cfg(feature = "lending")]

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

// Backend-owned buffer + counter state. The loan trampolines return
// pointers into this static buffer so the test can verify zero-copy
// semantics (no extra alloc). UnsafeCell because the backend mutates
// the buffer via the loan slot.
use core::cell::UnsafeCell;
struct LoanBuf(UnsafeCell<[u8; 256]>);
unsafe impl Sync for LoanBuf {}
static LOAN_BUF: LoanBuf = LoanBuf(UnsafeCell::new([0u8; 256]));
static LOAN_CALLS: AtomicUsize = AtomicUsize::new(0);
static COMMIT_CALLS: AtomicUsize = AtomicUsize::new(0);
static DISCARD_CALLS: AtomicUsize = AtomicUsize::new(0);
static LAST_COMMIT_LEN: AtomicUsize = AtomicUsize::new(0);
static LAST_COMMIT_BYTE0: AtomicUsize = AtomicUsize::new(0);
static FALLBACK_PUBLISH_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn ln_open(
    _: *const u8,
    _: u8,
    _: u32,
    _: *const u8,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xAB00usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_drive(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn ln_create_publisher(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0x5EEDusize as *mut c_void;
        (*out).can_loan_messages = true;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_destroy_pub(_: *mut NrosRmwPublisher) {}
unsafe extern "C" fn ln_publish_raw(
    _: *mut NrosRmwPublisher,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    FALLBACK_PUBLISH_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

unsafe extern "C" fn ln_pub_loan(
    _: *mut NrosRmwPublisher,
    requested_len: usize,
    out_buf: *mut *mut u8,
    out_cap: *mut usize,
    out_token: *mut *mut c_void,
) -> NrosRmwRet {
    LOAN_CALLS.fetch_add(1, Ordering::SeqCst);
    if requested_len > 256 {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    unsafe {
        let buf = LOAN_BUF.0.get();
        *out_buf = (*buf).as_mut_ptr();
        *out_cap = (*buf).len();
        // Encode the requested length in the token for the commit-side
        // assertion. Any non-null sentinel works for the test.
        *out_token = (requested_len as usize | 0x4242_0000) as *mut c_void;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn ln_pub_commit(
    _: *mut NrosRmwPublisher,
    token: *mut c_void,
    actual_len: usize,
) -> NrosRmwRet {
    COMMIT_CALLS.fetch_add(1, Ordering::SeqCst);
    LAST_COMMIT_LEN.store(actual_len, Ordering::SeqCst);
    LAST_COMMIT_BYTE0.store(unsafe { (*LOAN_BUF.0.get())[0] } as usize, Ordering::SeqCst);
    // Sanity-check the token tag.
    assert_eq!(
        (token as usize) & 0xFFFF_0000,
        0x4242_0000,
        "commit must receive the same token issued by pub_loan",
    );
    NROS_RMW_RET_OK
}

unsafe extern "C" fn ln_pub_discard(_: *mut NrosRmwPublisher, token: *mut c_void) {
    DISCARD_CALLS.fetch_add(1, Ordering::SeqCst);
    assert_eq!(
        (token as usize) & 0xFFFF_0000,
        0x4242_0000,
        "discard must receive the same token issued by pub_loan",
    );
}

// Shared no-op stubs for the unused vtable slots.
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
    open: ln_open,
    close: noop_close,
    drive_io: noop_drive,
    create_publisher: ln_create_publisher,
    destroy_publisher: noop_destroy_pub,
    publish_raw: ln_publish_raw,
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
    // Phase 124.A — native loan: backend exposes the 3 publisher slots,
    // forcing the runtime to route through them instead of the arena
    // fallback.
    pub_loan: Some(ln_pub_loan),
    pub_commit: Some(ln_pub_commit),
    pub_discard: Some(ln_pub_discard),
    sub_borrow: None,
    sub_release: None,
    service_server_available: None,
    try_recv_sequence: None,
    publish_streamed: None,
    ping_session: None,
    subscriber_supports_in_place: None,
    process_raw_in_place: None,
};

fn open_session() -> nros_rmw_cffi::CffiSession {
    let ret = unsafe { nros_rmw_cffi_register_named(c"ln_native".as_ptr(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK);
    let cfg = RmwConfig {
        mode: SessionMode::Client,
        locator: "tcp/127.0.0.1:7447",
        domain_id: 0,
        node_name: "ln_native_node",
        namespace: "",
        properties: &[],
    };
    CffiRmw::open_with_rmw("ln_native", &cfg).expect("open")
}

#[test]
fn native_loan_routes_through_vtable() {
    let mut session = open_session();
    let topic = TopicInfo::new("/ln", "std_msgs/msg/Int32", "RIHS01_ln");
    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");

    let pre_loan = LOAN_CALLS.load(Ordering::SeqCst);
    let pre_commit = COMMIT_CALLS.load(Ordering::SeqCst);
    let pre_publish = FALLBACK_PUBLISH_CALLS.load(Ordering::SeqCst);

    let mut slot = publisher
        .try_lend_slot(8)
        .expect("loan")
        .expect("backend exposes native loan → Some");
    // Write into the loaned buffer.
    slot.as_mut()[..5].copy_from_slice(b"ABCDE");
    slot.set_len(5);
    publisher.commit_slot(slot).expect("commit");

    assert_eq!(
        LOAN_CALLS.load(Ordering::SeqCst),
        pre_loan + 1,
        "pub_loan must fire exactly once",
    );
    assert_eq!(
        COMMIT_CALLS.load(Ordering::SeqCst),
        pre_commit + 1,
        "pub_commit must fire exactly once",
    );
    assert_eq!(LAST_COMMIT_LEN.load(Ordering::SeqCst), 5);
    assert_eq!(LAST_COMMIT_BYTE0.load(Ordering::SeqCst), b'A' as usize);
    assert_eq!(
        FALLBACK_PUBLISH_CALLS.load(Ordering::SeqCst),
        pre_publish,
        "native loan path must NOT trip the publish_raw fallback",
    );
}

#[test]
fn native_loan_drop_calls_discard() {
    let mut session = open_session();
    let topic = TopicInfo::new("/ln2", "std_msgs/msg/Int32", "RIHS01_ln").with_domain(0);
    let publisher = session
        .create_publisher(&topic, QosSettings::default())
        .expect("create publisher");

    let pre_loan = LOAN_CALLS.load(Ordering::SeqCst);
    let pre_commit = COMMIT_CALLS.load(Ordering::SeqCst);
    let pre_discard = DISCARD_CALLS.load(Ordering::SeqCst);

    {
        let mut slot = publisher
            .try_lend_slot(16)
            .expect("loan")
            .expect("native loan Some");
        slot.as_mut()[0] = 0x42;
        // Drop without commit → pub_discard fires.
    }

    assert_eq!(LOAN_CALLS.load(Ordering::SeqCst), pre_loan + 1);
    assert_eq!(
        COMMIT_CALLS.load(Ordering::SeqCst),
        pre_commit,
        "drop must NOT commit",
    );
    assert_eq!(
        DISCARD_CALLS.load(Ordering::SeqCst),
        pre_discard + 1,
        "drop MUST discard the unsent loan",
    );
}
