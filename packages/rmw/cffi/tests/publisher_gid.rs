//! The phase-467 RMW gap-closure design study's Q1 and Row 7 — the two
//! publisher accessors whose answer depends on whether a vtable slot is NULL.
//!
//! Both are about the SAME failure shape and are tested together for that
//! reason: an accessor over an optional slot must say "this backend has not
//! answered" in a way a caller can tell apart from an answer.
//!
//! * `get_gid` — an all-zero gid is what an uninitialised `rmw_gid_t` holds,
//!   so returning one on the NULL-slot path would make "no identity" read
//!   exactly like an identity.
//! * `assert_liveliness` — `Ok(())` from a call that put nothing on the wire
//!   is the same lie one method over, and it is what the zenoh shim answered
//!   until this study. But `Ok(())` is also the RIGHT answer when the
//!   publisher's liveliness kind is AUTOMATIC or NONE, because then there was
//!   nothing to assert. `rmw_vtable.h` has stated that split since phase 108
//!   and nothing implemented it: the NULL-slot arm answered `Unsupported`
//!   whatever the caller had asked for.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{
    Publisher as _, QoSLivelinessPolicy, QoSProfile, RmwConfig, Session as _, SessionMode,
    TopicInfo, TransportError,
};
use nros_rmw_cffi::{
    CffiRmw, EMPTY_VTABLE, NROS_RMW_RET_ERROR, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwEventCallback, NrosRmwEventKind, NrosRmwGid, NrosRmwNode,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService, NrosRmwSession,
    NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable, nros_rmw_cffi_register_named,
};

/// What the filled-slot backend below reports. Deliberately NOT all-zero and
/// NOT all-the-same-byte: a body that memset the struct, or one that wrote the
/// wrong number of bytes, would still pass against a uniform pattern.
const BACKEND_GID: [u8; 24] = [
    0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

static ASSERT_LIVELINESS_CALLS: AtomicUsize = AtomicUsize::new(0);

// ---- the two slots under test -------------------------------------------

unsafe extern "C" fn filled_get_gid(
    _: *const NrosRmwPublisher,
    gid: *mut NrosRmwGid,
) -> NrosRmwRet {
    unsafe { (*gid).data = BACKEND_GID };
    NROS_RMW_RET_OK
}

/// A backend that HAS the slot and fails the call — distinct from a NULL slot,
/// and the reason the accessor cannot collapse both onto `Unsupported`.
unsafe extern "C" fn failing_get_gid(_: *const NrosRmwPublisher, _: *mut NrosRmwGid) -> NrosRmwRet {
    NROS_RMW_RET_ERROR
}

unsafe extern "C" fn real_assert_liveliness(_: *const NrosRmwPublisher) -> NrosRmwRet {
    ASSERT_LIVELINESS_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
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

/// Every REQUIRED slot filled — `nros_rmw_cffi_register` refuses an all-NULL
/// vtable (issue 0349) — and both optional slots under test left NULL, which
/// is the state uORB and XRCE actually ship.
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

static GID_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_gid_for_publisher: Some(filled_get_gid),
    publisher_assert_liveliness: Some(real_assert_liveliness),
    ..BASE
};

static FAILING_GID_VTABLE: NrosRmwVtable = NrosRmwVtable {
    get_gid_for_publisher: Some(failing_get_gid),
    ..BASE
};

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

fn manual_qos() -> QoSProfile {
    QoSProfile {
        liveliness_kind: QoSLivelinessPolicy::ManualByTopic,
        ..QoSProfile::default()
    }
}

// NOTE ON TEST COUNT: `MAX_NODES` is 4 in this build (`NROS_RMW_MAX_NODES`,
// default 4), and the node-slot table is a PROCESS-WIDE static shared by every
// test in this binary — a session that claims a slot never releases it. So the
// three vtables above get three tests, each asserting everything it can from
// one node identity; a fourth registered backend here exhausts the table and
// the next `create_publisher` fails `ConnectionFailed`, which reads like a
// backend fault and is not one. Measured, not assumed: a five-test first draft
// failed exactly that way.

#[test]
fn a_null_gid_slot_answers_unsupported_and_never_zeros() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"gid_silent".as_ptr(), &SILENT_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("gid_silent", &config("gid_silent_node")).expect("open");
    let publisher = session
        .create_publisher(
            &TopicInfo::new("/gid", "std_msgs/msg/Int32", "RIHS01_gid"),
            QoSProfile::default(),
        )
        .expect("create publisher");

    // NOT `Ok([0u8; 24])`. The `rmw_gid_t` the accessor stages is zeroed
    // before the call, so "return it anyway" is one missing early-return
    // away, and the caller could not tell the two apart.
    assert_eq!(publisher.get_gid(), Err(TransportError::Unsupported));

    // ---- Row 7, on the same NULL-slot backend --------------------------
    //
    // The default profile's liveliness kind is NONE: nothing to assert, so
    // `Ok(())` is the whole truth and not a claim about the wire.
    assert_eq!(publisher.assert_liveliness(), Ok(()));

    // MANUAL_BY_TOPIC: the caller asked for a per-topic lease this backend
    // has no way to renew. `Ok(())` here would report that an assertion
    // reached a peer when none did — the answer `rmw_vtable.h` has
    // prescribed since phase 108 and nothing implemented until now.
    let manual = session
        .create_publisher(
            &TopicInfo::new("/gid_manual", "std_msgs/msg/Int32", "RIHS01_gid"),
            manual_qos(),
        )
        .expect("create publisher");
    assert_eq!(manual.assert_liveliness(), Err(TransportError::Unsupported));
}

#[test]
fn a_filled_gid_slot_answers_its_own_bytes() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"gid_filled".as_ptr(), &GID_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("gid_filled", &config("gid_filled_node")).expect("open");
    let publisher = session
        .create_publisher(
            &TopicInfo::new("/gid", "std_msgs/msg/Int32", "RIHS01_gid"),
            manual_qos(),
        )
        .expect("create publisher");

    assert_eq!(publisher.get_gid(), Ok(BACKEND_GID));

    // ---- Row 7: a REAL slot is called whatever the kind ----------------
    let before = ASSERT_LIVELINESS_CALLS.load(Ordering::SeqCst);
    assert_eq!(publisher.assert_liveliness(), Ok(()));
    assert_eq!(
        ASSERT_LIVELINESS_CALLS.load(Ordering::SeqCst),
        before + 1,
        "the kind split must gate only the NULL-slot arm, never a real one",
    );
}

#[test]
fn a_gid_slot_that_fails_is_not_reported_as_unsupported() {
    assert_eq!(
        unsafe { nros_rmw_cffi_register_named(c"gid_failing".as_ptr(), &FAILING_GID_VTABLE) },
        NROS_RMW_RET_OK,
    );
    let mut session =
        CffiRmw::open_with_rmw("gid_failing", &config("gid_failing_node")).expect("open");
    let publisher = session
        .create_publisher(
            &TopicInfo::new("/gid", "std_msgs/msg/Int32", "RIHS01_gid"),
            QoSProfile::default(),
        )
        .expect("create publisher");

    // "the backend cannot do this" and "the backend tried and failed" are
    // different facts, and only the first is a property of the image.
    let verdict = publisher.get_gid();
    assert!(verdict.is_err(), "a failing slot must not report a gid");
    assert_ne!(verdict, Err(TransportError::Unsupported));
}
