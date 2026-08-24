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
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos,
    NrosRmwRet, NrosRmwService, NrosRmwSession, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named,
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
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
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
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe {
        (*out).backend_data = 0xCAFEusize as *mut c_void;
        (*out).can_loan_messages = false;
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn publish_raw(
    _: *const NrosRmwPublisher,
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
unsafe extern "C" fn noop_dsub(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recv(
    _: *const NrosRmwSubscription,
    _: *mut u8,
    _: usize,
    _: *mut usize,
    _: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — this stub always FAILED (-1); it still does,
    // now as a named status rather than a negative number.
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_hasd(_: *mut NrosRmwSubscription, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_csrv(
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
unsafe extern "C" fn noop_dsrv(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_recvreq(
    _: *const NrosRmwService,
    _: *mut u8,
    _: usize,
    _: *mut i64,
    _: *mut usize,
    _: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — this stub always FAILED; still does, named.
    NROS_RMW_RET_ERROR
}
unsafe extern "C" fn noop_hasreq(_: *mut NrosRmwService, has: *mut bool) -> NrosRmwRet {
    unsafe { *has = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn noop_reply(
    _: *const NrosRmwService,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn noop_ccli(
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
unsafe extern "C" fn noop_alv(_: *const NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
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
    publisher_assert_liveliness: Some(noop_alv),
    // Phase 124.A.3 — NULL pub_loan: runtime falls back to arena.
    ..EMPTY_VTABLE
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
