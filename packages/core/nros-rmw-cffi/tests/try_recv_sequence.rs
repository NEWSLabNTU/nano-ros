//! Phase 124.D.4 — `CffiSubscriber::try_recv_sequence` routing test.
//!
//! Two scenarios:
//!
//! 1. Backend exposes a native batch slot. The runtime calls it
//!    once, the stub writes 8 messages with growing payloads, and
//!    the caller observes all 8 messages back-to-back with correct
//!    per-slot lengths.
//!
//! 2. Backend leaves the slot NULL. The runtime falls back to a
//!    `try_recv_raw` loop. A counter-driven stub `try_recv_raw`
//!    feeds 8 messages on consecutive calls and then reports empty;
//!    the loop terminates at the correct count.
//!
//! Both paths share the user-facing call shape; the test asserts
//! they deliver identical content.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};

use nros_rmw::{Session, SessionMode, Subscriber, TopicInfo};
use nros_rmw_cffi::{
    NROS_RMW_RET_NO_DATA, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwEventCallback,
    NrosRmwEventKind, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwServiceClient,
    NrosRmwServiceServer, NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable,
    nros_rmw_cffi_register_named,
};

const PER_MSG_CAP: usize = 32;
const QUEUE: [&[u8]; 8] = [
    b"m0",
    b"msg-01",
    b"sample-002",
    b"event-0003",
    b"datum-00004",
    b"reading-000005",
    b"telemetry-0000006",
    b"observation-0000007",
];

// ---- Stub backend wiring ----

static SEQ_CALLS_NATIVE: AtomicUsize = AtomicUsize::new(0);
static RAW_CURSOR: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn stub_open(
    _: *const u8,
    _: u8,
    _: u32,
    _: *const u8,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_create_publisher(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) {}
unsafe extern "C" fn stub_publish_raw(
    _: *mut NrosRmwPublisher,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_subscriber(
    _: *mut NrosRmwSession,
    _: *const u8,
    _: *const u8,
    _: *const u8,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwSubscriber,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xa5a5usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_subscriber(_: *mut NrosRmwSubscriber) {}

// `try_recv_raw` stub: feed the i-th queue entry on the i-th call.
unsafe extern "C" fn stub_try_recv_raw(
    _: *mut NrosRmwSubscriber,
    buf: *mut u8,
    buf_len: usize,
) -> i32 {
    let cursor = RAW_CURSOR.fetch_add(1, Ordering::SeqCst);
    if cursor >= QUEUE.len() {
        return 0;
    }
    let msg = QUEUE[cursor];
    let copy = msg.len().min(buf_len);
    unsafe { core::ptr::copy_nonoverlapping(msg.as_ptr(), buf, copy) };
    copy as i32
}
unsafe extern "C" fn stub_try_recv_raw_no_data(
    _: *mut NrosRmwSubscriber,
    _: *mut u8,
    _: usize,
) -> i32 {
    NROS_RMW_RET_NO_DATA
}
unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscriber) -> i32 {
    if RAW_CURSOR.load(Ordering::SeqCst) < QUEUE.len() {
        1
    } else {
        0
    }
}
unsafe extern "C" fn stub_create_service_server(
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
unsafe extern "C" fn stub_destroy_service_server(_: *mut NrosRmwServiceServer) {}
unsafe extern "C" fn stub_try_recv_request(
    _: *mut NrosRmwServiceServer,
    _: *mut u8,
    _: usize,
    _: *mut i64,
) -> i32 {
    0
}
unsafe extern "C" fn stub_has_request(_: *mut NrosRmwServiceServer) -> i32 {
    0
}
unsafe extern "C" fn stub_send_reply(
    _: *mut NrosRmwServiceServer,
    _: i64,
    _: *const u8,
    _: usize,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_service_client(
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
unsafe extern "C" fn stub_destroy_service_client(_: *mut NrosRmwServiceClient) {}
unsafe extern "C" fn stub_call_raw(
    _: *mut NrosRmwServiceClient,
    _: *const u8,
    _: usize,
    _: *mut u8,
    _: usize,
) -> i32 {
    -1
}
unsafe extern "C" fn stub_reg_sub_event(
    _: *mut NrosRmwSubscriber,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_reg_pub_event(
    _: *mut NrosRmwPublisher,
    _: NrosRmwEventKind,
    _: u32,
    _: NrosRmwEventCallback,
    _: *mut c_void,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_assert_liveliness(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}

// Native batch: write all 8 messages in one call.
unsafe extern "C" fn stub_try_recv_sequence(
    _: *mut NrosRmwSubscriber,
    buf: *mut u8,
    per_msg_cap: usize,
    max_msgs: usize,
    out_lens: *mut usize,
) -> i32 {
    SEQ_CALLS_NATIVE.fetch_add(1, Ordering::SeqCst);
    let to_emit = QUEUE.len().min(max_msgs);
    for (i, msg) in QUEUE.iter().take(to_emit).enumerate() {
        let copy = msg.len().min(per_msg_cap);
        unsafe {
            core::ptr::copy_nonoverlapping(msg.as_ptr(), buf.add(i * per_msg_cap), copy);
            *out_lens.add(i) = copy;
        }
    }
    to_emit as i32
}

fn make_vtable(native_batch: bool) -> NrosRmwVtable {
    NrosRmwVtable {
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscriber_event: stub_reg_sub_event,
        register_publisher_event: stub_reg_pub_event,
        assert_publisher_liveliness: stub_assert_liveliness,
        next_deadline_ms: None,
        set_wake_callback: None,
        pub_loan: None,
        pub_commit: None,
        pub_discard: None,
        sub_borrow: None,
        sub_release: None,
        service_server_available: None,
        try_recv_sequence: if native_batch {
            Some(stub_try_recv_sequence)
        } else {
            None
        },
        publish_streamed: None,
        ping_session: None,
        subscriber_supports_in_place: None,
        process_raw_in_place: None,
    }
}

static VTABLE_NATIVE: NrosRmwVtable = make_vtable_native();
static VTABLE_FALLBACK: NrosRmwVtable = make_vtable_fallback();
static VTABLE_NO_DATA: NrosRmwVtable = make_vtable_no_data();

const fn make_vtable_native() -> NrosRmwVtable {
    NrosRmwVtable {
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscriber_event: stub_reg_sub_event,
        register_publisher_event: stub_reg_pub_event,
        assert_publisher_liveliness: stub_assert_liveliness,
        next_deadline_ms: None,
        set_wake_callback: None,
        pub_loan: None,
        pub_commit: None,
        pub_discard: None,
        sub_borrow: None,
        sub_release: None,
        service_server_available: None,
        try_recv_sequence: Some(stub_try_recv_sequence),
        publish_streamed: None,
        ping_session: None,
        subscriber_supports_in_place: None,
        process_raw_in_place: None,
    }
}

const fn make_vtable_fallback() -> NrosRmwVtable {
    NrosRmwVtable {
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscriber_event: stub_reg_sub_event,
        register_publisher_event: stub_reg_pub_event,
        assert_publisher_liveliness: stub_assert_liveliness,
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
        subscriber_supports_in_place: None,
        process_raw_in_place: None,
    }
}

const fn make_vtable_no_data() -> NrosRmwVtable {
    NrosRmwVtable {
        open: stub_open,
        close: stub_close,
        drive_io: stub_drive_io,
        create_publisher: stub_create_publisher,
        destroy_publisher: stub_destroy_publisher,
        publish_raw: stub_publish_raw,
        create_subscriber: stub_create_subscriber,
        destroy_subscriber: stub_destroy_subscriber,
        try_recv_raw: stub_try_recv_raw_no_data,
        has_data: stub_has_data,
        create_service_server: stub_create_service_server,
        destroy_service_server: stub_destroy_service_server,
        try_recv_request: stub_try_recv_request,
        has_request: stub_has_request,
        send_reply: stub_send_reply,
        create_service_client: stub_create_service_client,
        destroy_service_client: stub_destroy_service_client,
        call_raw: stub_call_raw,
        send_request_raw: None,
        try_recv_reply_raw: None,
        register_subscriber_event: stub_reg_sub_event,
        register_publisher_event: stub_reg_pub_event,
        assert_publisher_liveliness: stub_assert_liveliness,
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
        subscriber_supports_in_place: None,
        process_raw_in_place: None,
    }
}

fn open_subscriber(name: &str, vtable: &'static NrosRmwVtable) -> nros_rmw_cffi::CffiSubscriber {
    let cname = format!("{name}\0");
    let ret = unsafe { nros_rmw_cffi_register_named(cname.as_ptr() as *const _, vtable) };
    assert_eq!(ret, NROS_RMW_RET_OK);
    // Each test uses its own backend name; route through the named
    // open path so we don't depend on default-vtable semantics
    // when other tests in the same binary register competing
    // backends.
    let mut session = nros_rmw_cffi::CffiSession::open_named(
        name,
        "tcp/127.0.0.1:7447",
        SessionMode::Client as u8,
        0,
        "stub_node",
    )
    .expect("open_named");
    let _ = &session as &dyn core::any::Any; // silence unused if `Session` unused
    let info = TopicInfo::new("/burst", "example/Burst", "RIHS01_burst");
    let qos = nros_rmw::QosSettings::default();
    let sub = session.create_subscriber(&info, qos).expect("create_sub");
    core::mem::forget(session);
    sub
}

#[test]
fn try_recv_sequence_native_batch() {
    SEQ_CALLS_NATIVE.store(0, Ordering::SeqCst);
    RAW_CURSOR.store(0, Ordering::SeqCst);
    let mut sub = open_subscriber("tb_seq_native", &VTABLE_NATIVE);

    let mut buf = [0u8; 8 * PER_MSG_CAP];
    let mut lens = [0usize; 8];
    let count = sub
        .try_recv_sequence(&mut buf, PER_MSG_CAP, 8, &mut lens)
        .expect("try_recv_sequence");
    assert_eq!(count, QUEUE.len(), "expected all 8 messages in one call");
    assert_eq!(
        SEQ_CALLS_NATIVE.load(Ordering::SeqCst),
        1,
        "native slot should be called exactly once"
    );
    for (i, expected) in QUEUE.iter().enumerate() {
        assert_eq!(lens[i], expected.len(), "lens[{i}] mismatch");
        assert_eq!(
            &buf[i * PER_MSG_CAP..i * PER_MSG_CAP + lens[i]],
            *expected,
            "payload[{i}] mismatch"
        );
    }
}

#[test]
fn try_recv_sequence_loop_fallback() {
    RAW_CURSOR.store(0, Ordering::SeqCst);
    let mut sub = open_subscriber("tb_seq_fallback", &VTABLE_FALLBACK);

    let mut buf = [0u8; 8 * PER_MSG_CAP];
    let mut lens = [0usize; 8];
    let count = sub
        .try_recv_sequence(&mut buf, PER_MSG_CAP, 8, &mut lens)
        .expect("try_recv_sequence");
    assert_eq!(count, QUEUE.len(), "expected all 8 messages via loop");
    assert_eq!(
        RAW_CURSOR.load(Ordering::SeqCst),
        QUEUE.len(),
        "fallback should call try_recv_raw exactly 8 times (loop stops at max_msgs)"
    );
    for (i, expected) in QUEUE.iter().enumerate() {
        assert_eq!(lens[i], expected.len(), "lens[{i}] mismatch");
        assert_eq!(
            &buf[i * PER_MSG_CAP..i * PER_MSG_CAP + lens[i]],
            *expected,
            "payload[{i}] mismatch"
        );
    }
}

#[test]
fn try_recv_raw_no_data_maps_to_none() {
    let mut sub = open_subscriber("tb_seq_no_data", &VTABLE_NO_DATA);

    let mut buf = [0u8; PER_MSG_CAP];
    let received = sub.try_recv_raw(&mut buf).expect("NO_DATA is not an error");

    assert_eq!(received, None);
}

#[test]
fn try_recv_sequence_rejects_zero_per_msg_cap() {
    RAW_CURSOR.store(0, Ordering::SeqCst);
    let mut sub = open_subscriber("tb_seq_zero_cap", &VTABLE_FALLBACK);

    let mut buf = [0u8; 64];
    let mut lens = [0usize; 4];
    let count = sub
        .try_recv_sequence(&mut buf, 0, 4, &mut lens)
        .expect("zero cap returns Ok(0)");
    assert_eq!(count, 0, "per_msg_cap=0 should drain zero messages");

    // No `try_recv_raw` calls fired because the loop short-circuits.
    assert_eq!(RAW_CURSOR.load(Ordering::SeqCst), 0);
}

// `make_vtable` is the runtime constructor used by ad-hoc tests; we
// just need to ensure it stays compileable when const-eval isn't an
// option. Keeping the function around documents the alternative
// surface even though the active tests use the two `const fn`
// statics above.
#[allow(dead_code)]
fn _make_vtable_smoke() -> NrosRmwVtable {
    make_vtable(false)
}
