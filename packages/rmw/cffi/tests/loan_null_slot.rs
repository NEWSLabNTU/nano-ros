//! issue 0814 — a backend whose vtable has no `borrow_loaned_message` cannot
//! lend, and `try_lend_slot` must say so PERMANENTLY, the same way in every
//! build.
//!
//! That is a static fact about the publisher, so it cannot depend on the
//! build. Until option 2 of 0814 it did: a heap-free build refused, while an
//! `alloc` build handed out a staging `Box` + `Vec`, had the caller encode
//! into it exactly as into its own buffer, then `publish_raw`'d it — no copy
//! removed, an allocation pair added, and `can_loan_messages` (derived from
//! the same slot) saying `false` the whole time. This file was the heap-free
//! twin of `loan_fallback.rs`, which tested that staging; both are now one
//! contract, and this file runs it with and without `alloc`.
//!
//! The answer must also be an ERROR, not `Ok(None)`. Every caller above reads
//! `Ok(None)` as "no slot RIGHT NOW, retry": `EmbeddedRawPublisher::try_loan`
//! maps it to `LoanError::WouldBlock`, `loan_with_timeout` then spins the
//! executor until its whole budget is gone, and `LoanFuture` returns `Pending`
//! after a self-wake — a hot loop that can never resolve.

#![cfg(feature = "lending")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{
    QoSProfile, RmwConfig, Session as _, SessionMode, SlotLending, TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    CffiPublisher, CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

static PUBLISH_CALLS: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    _options: *const NrosRmwSessionOptions,
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
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
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
    data: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    // Only the CALL COUNT matters here: the contract under test is that a
    // refused loan publishes nothing, so the payload is never inspected.
    let _ = data;
    PUBLISH_CALLS.fetch_add(1, Ordering::SeqCst);
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
    // Phase 376 W3.d step A — this stub always FAILED (-1); it still does,
    // now as a named status rather than a negative number.
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
    // NULL `borrow_loaned_message` — Cyclone, XRCE and uORB's shape.
    ..EMPTY_VTABLE
};

fn publisher_on(locator: &'static str, node: &'static str, topic: &'static str) -> CffiPublisher {
    // nextest forks per test, so each test re-registers into its own registry.
    let ret = unsafe { nros_rmw_cffi_register_named(c"lna_arena".as_ptr(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK, "register");

    let cfg = RmwConfig {
        mode: SessionMode::Client,
        locator,
        domain_id: 0,
        node_name: node,
        namespace: "",
        properties: &[],
    };
    let mut session = CffiRmw::open_with_rmw("lna_arena", &cfg).expect("open");
    let info = TopicInfo::new(topic, "std_msgs/msg/Int32", "RIHS01_lna");
    session
        .create_publisher(&info, QoSProfile::default())
        .expect("create publisher")
}

/// The contract: a publisher whose backend cannot lend reports a PERMANENT
/// error, never `Ok(None)` and never a slot — with or without `alloc`.
///
/// `Ok(None)` is the transient answer — it is what a native backend returns
/// for `NROS_RMW_RET_WOULD_BLOCK`, i.e. "the slot is busy, ask again". This
/// arm can never become available, so it must not borrow that spelling.
#[test]
fn a_null_loan_slot_is_a_permanent_error_not_would_block() {
    let publisher = publisher_on("tcp/127.0.0.1:7451", "lna_node", "/lna");

    let outcome = publisher.try_lend_slot(8);

    match outcome {
        Err(TransportError::Unsupported) => {}
        Ok(None) => panic!(
            "a NULL loan slot returned Ok(None) — the transient spelling. \
             `try_loan` maps that to LoanError::WouldBlock, so `loan_with_timeout` \
             burns its whole budget and `LoanFuture` self-wakes forever on a \
             condition that can never clear."
        ),
        Ok(Some(_)) => panic!(
            "a NULL loan slot handed out a slot — the backend cannot lend, so \
             this can only be a staging buffer, which removes no copy (issue 0814)"
        ),
        Err(other) => panic!(
            "a NULL loan slot reported {other:?}; the caller needs to be able to \
             tell PERMANENT from retryable, which is what Unsupported says"
        ),
    }
}

/// A refused loan must stay refused. If the answer were transient, a caller
/// would be right to retry; it is not, so a second identical call must give
/// the identical permanent answer rather than eventually succeeding.
#[test]
fn a_null_loan_slot_refusal_is_stable_across_retries() {
    let publisher = publisher_on("tcp/127.0.0.1:7452", "lna_node2", "/lna2");

    for attempt in 0..4 {
        assert!(
            matches!(publisher.try_lend_slot(8), Err(TransportError::Unsupported)),
            "attempt {attempt} changed the answer — the condition is a static fact \
             about the vtable, so it cannot"
        );
    }
}

/// The refusal must not have published anything. A caller that reads the error
/// and falls back to `publish_raw` needs to know the failed loan was inert.
#[test]
fn refused_loan_publishes_nothing() {
    let publisher = publisher_on("tcp/127.0.0.1:7453", "lna_node3", "/lna3");

    let before = PUBLISH_CALLS.load(Ordering::SeqCst);
    let _ = publisher.try_lend_slot(8);
    assert_eq!(
        PUBLISH_CALLS.load(Ordering::SeqCst),
        before,
        "a refused loan must emit no publish_raw"
    );
}
