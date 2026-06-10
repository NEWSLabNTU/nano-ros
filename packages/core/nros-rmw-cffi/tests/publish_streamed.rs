//! Phase 124.E.4 — `CffiPublisher::publish_streamed` routing test.
//!
//! Two scenarios:
//!
//! 1. **Native slot.** Backend exposes `publish_streamed`. The stub
//!    receives the callbacks, asks for the total length, drains the
//!    chunk callback into a recording buffer, and reports back to the
//!    test. The runtime makes ONE vtable call regardless of how many
//!    chunks the callback delivers.
//!
//! 2. **Staging-buffer fallback.** Backend leaves the slot NULL. The
//!    runtime fills a 4 KiB stack buffer via the chunk callback and
//!    falls through to `publish_raw`. Wire bytes are recorded by a
//!    stub `publish_raw` and compared against the chunked input.
//!
//! Both paths deliver byte-identical wire output.
#![cfg(feature = "alloc")]

use core::{
    ffi::c_void,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::sync::Mutex;

use nros_rmw::{Publisher as _, QosSettings, Session, SessionMode, TopicInfo};
use nros_rmw_cffi::{
    NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED, NrosRmwEventCallback, NrosRmwEventKind,
    NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwServiceClient, NrosRmwServiceServer,
    NrosRmwSession, NrosRmwSubscriber, NrosRmwVtable, nros_rmw_cffi_register_named,
};

const PAYLOAD: &[u8] = b"streamed-publish-payload-0123456789ABCDEF";

// Recording buffers for the two scenarios. Mutex-protected because
// `extern "C"` callbacks are otherwise unsafe to mutate.
static NATIVE_RECORD: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static FALLBACK_RECORD: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static NATIVE_CALLS: AtomicUsize = AtomicUsize::new(0);
static FALLBACK_CALLS: AtomicUsize = AtomicUsize::new(0);

// ----- stubs reused across both vtables --------------------------------------

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
    out: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = 0xa5a5usize as *mut c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) {}

// `publish_raw`: record bytes into `FALLBACK_RECORD`. Both vtables
// share the same stub; the native vtable's `publish_streamed`
// short-circuits before `publish_raw` ever fires.
unsafe extern "C" fn stub_publish_raw(
    _: *mut NrosRmwPublisher,
    data: *const u8,
    len: usize,
) -> NrosRmwRet {
    let slice = unsafe { core::slice::from_raw_parts(data, len) };
    let mut rec = FALLBACK_RECORD.lock().unwrap();
    rec.extend_from_slice(slice);
    FALLBACK_CALLS.fetch_add(1, Ordering::SeqCst);
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_create_subscriber(
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
unsafe extern "C" fn stub_destroy_subscriber(_: *mut NrosRmwSubscriber) {}
unsafe extern "C" fn stub_try_recv_raw(_: *mut NrosRmwSubscriber, _: *mut u8, _: usize) -> i32 {
    0
}
unsafe extern "C" fn stub_has_data(_: *mut NrosRmwSubscriber) -> i32 {
    0
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

// Native streamed slot: invoke the callbacks like a real backend
// would and record the streamed bytes.
unsafe extern "C" fn stub_publish_streamed(
    _: *mut NrosRmwPublisher,
    size_cb: unsafe extern "C" fn(out_total_len: *mut usize, user_ctx: *mut c_void),
    chunk_cb: unsafe extern "C" fn(
        out_buf: *mut u8,
        cap: usize,
        out_written: *mut usize,
        user_ctx: *mut c_void,
    ),
    user_ctx: *mut c_void,
) -> NrosRmwRet {
    NATIVE_CALLS.fetch_add(1, Ordering::SeqCst);
    let mut total = 0usize;
    unsafe { size_cb(&mut total as *mut usize, user_ctx) };
    let mut buf = vec![0u8; total];
    let mut filled = 0usize;
    while filled < total {
        let mut written = 0usize;
        unsafe {
            chunk_cb(
                buf.as_mut_ptr().add(filled),
                total - filled,
                &mut written as *mut usize,
                user_ctx,
            );
        }
        if written == 0 {
            break;
        }
        filled += written;
    }
    NATIVE_RECORD
        .lock()
        .unwrap()
        .extend_from_slice(&buf[..filled]);
    NROS_RMW_RET_OK
}

const fn make_base() -> NrosRmwVtable {
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

static VTABLE_NATIVE: NrosRmwVtable = {
    let mut v = make_base();
    v.publish_streamed = Some(stub_publish_streamed);
    v
};

static VTABLE_FALLBACK: NrosRmwVtable = make_base();

fn open_publisher(name: &str, vt: &'static NrosRmwVtable) -> nros_rmw_cffi::CffiPublisher {
    let cname = format!("{name}\0");
    let ret = unsafe { nros_rmw_cffi_register_named(cname.as_ptr() as *const _, vt) };
    assert_eq!(ret, NROS_RMW_RET_OK);
    let mut session = nros_rmw_cffi::CffiSession::open_named(
        name,
        "tcp/127.0.0.1:7447",
        SessionMode::Client as u8,
        0,
        "stub_node",
    )
    .expect("open_named");
    let info = TopicInfo::new("/streamed", "example/Streamed", "RIHS01_streamed");
    let qos = QosSettings::default();
    let pub_ = session.create_publisher(&info, qos).expect("create_pub");
    core::mem::forget(session);
    pub_
}

#[test]
fn publish_streamed_native_path() {
    NATIVE_RECORD.lock().unwrap().clear();
    FALLBACK_RECORD.lock().unwrap().clear();
    NATIVE_CALLS.store(0, Ordering::SeqCst);
    FALLBACK_CALLS.store(0, Ordering::SeqCst);

    let pub_ = open_publisher("tb_stream_native", &VTABLE_NATIVE);

    struct Ctx<'a> {
        bytes: &'a [u8],
        cursor: usize,
    }
    unsafe extern "C" fn sz(out: *mut usize, ctx: *mut c_void) {
        unsafe {
            let c = &*(ctx as *const Ctx);
            *out = c.bytes.len();
        }
    }
    // Emit one chunk of 13 bytes, then drain the rest in one shot.
    unsafe extern "C" fn ch(
        out_buf: *mut u8,
        cap: usize,
        out_written: *mut usize,
        ctx: *mut c_void,
    ) {
        unsafe {
            let c = &mut *(ctx as *mut Ctx);
            let remaining = c.bytes.len() - c.cursor;
            let n = cap.min(remaining).min(13);
            core::ptr::copy_nonoverlapping(c.bytes.as_ptr().add(c.cursor), out_buf, n);
            c.cursor += n;
            *out_written = n;
        }
    }

    let mut ctx = Ctx {
        bytes: PAYLOAD,
        cursor: 0,
    };
    unsafe {
        pub_.publish_streamed(sz, ch, &mut ctx as *mut Ctx as *mut c_void)
            .expect("publish_streamed");
    }

    let rec = NATIVE_RECORD.lock().unwrap();
    assert_eq!(&rec[..], PAYLOAD);
    assert_eq!(NATIVE_CALLS.load(Ordering::SeqCst), 1, "one vtable call");
    assert_eq!(
        FALLBACK_CALLS.load(Ordering::SeqCst),
        0,
        "native slot must not fall through to publish_raw"
    );
}

#[test]
fn publish_streamed_fallback_path() {
    NATIVE_RECORD.lock().unwrap().clear();
    FALLBACK_RECORD.lock().unwrap().clear();
    NATIVE_CALLS.store(0, Ordering::SeqCst);
    FALLBACK_CALLS.store(0, Ordering::SeqCst);

    let pub_ = open_publisher("tb_stream_fallback", &VTABLE_FALLBACK);

    struct Ctx<'a> {
        bytes: &'a [u8],
        cursor: usize,
    }
    unsafe extern "C" fn sz(out: *mut usize, ctx: *mut c_void) {
        unsafe {
            let c = &*(ctx as *const Ctx);
            *out = c.bytes.len();
        }
    }
    unsafe extern "C" fn ch(
        out_buf: *mut u8,
        cap: usize,
        out_written: *mut usize,
        ctx: *mut c_void,
    ) {
        unsafe {
            let c = &mut *(ctx as *mut Ctx);
            let remaining = c.bytes.len() - c.cursor;
            let n = cap.min(remaining).min(7);
            core::ptr::copy_nonoverlapping(c.bytes.as_ptr().add(c.cursor), out_buf, n);
            c.cursor += n;
            *out_written = n;
        }
    }

    let mut ctx = Ctx {
        bytes: PAYLOAD,
        cursor: 0,
    };
    unsafe {
        pub_.publish_streamed(sz, ch, &mut ctx as *mut Ctx as *mut c_void)
            .expect("publish_streamed fallback");
    }

    let rec = FALLBACK_RECORD.lock().unwrap();
    assert_eq!(&rec[..], PAYLOAD, "fallback wire bytes match input");
    assert_eq!(
        NATIVE_CALLS.load(Ordering::SeqCst),
        0,
        "no native call expected on fallback path"
    );
    assert_eq!(
        FALLBACK_CALLS.load(Ordering::SeqCst),
        1,
        "exactly one publish_raw at end of stream"
    );
}
