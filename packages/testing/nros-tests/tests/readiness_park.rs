//! phase-521 W3 (RFC-0113 D3/D4) — a tier parks on its session's WAITABLE
//! handle and is woken by data arriving, with no wake callback installed.
//!
//! The backend is a stub registered under its own name. Its readiness is one
//! end of a Unix socket pair (`ReadinessSource::Waitable`), its
//! `set_wake_callback` slot is NULL, and its `drive_io` records the timeout it
//! was asked to block for and drains the socket. A writer thread puts a byte
//! on the other end partway through a long `spin_once`.
//!
//! What is asserted:
//!
//! 1. The spin returns after the write and long before its budget: the park
//!    ended on the handle, which only the platform's multiplexed park can
//!    see (nothing signals the executor's wake object).
//! 2. `drive_io` was only ever asked for a NON-blocking drive: the tier never
//!    waited inside the transport (RFC-0113 D2.6).
//! 3. Negative control: the same stub reporting no readiness form is driven
//!    for the spin's whole budget, i.e. the receive-blocking wait that W6
//!    retires. Without it, (2) could pass because nothing ever blocks.
//!
//! Run: `cargo nextest run -p nros-tests --features trigger-test --test readiness_park`.

#![cfg(all(feature = "trigger-test", unix))]

// The POSIX platform port (the wake object and its handle park) is force-linked
// by `nros_tests`'s own `lib.rs`; `as _` keeps that reference without a name.
use nros_tests as _;

use std::{
    io::Write,
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{
        Mutex,
        atomic::{AtomicI32, AtomicU8, Ordering},
    },
    time::{Duration, Instant},
};

use nros_node::executor::*;
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_READINESS_NONE, NROS_RMW_READINESS_WAITABLE, NROS_RMW_RET_OK,
    NROS_RMW_RET_UNSUPPORTED, NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet,
    NrosRmwService, NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named, rmw_session_t,
};

/// When the writer puts its byte on the socket, after the spin entered.
const WRITE_AFTER: Duration = Duration::from_millis(100);
/// The spin's budget: far longer than `WRITE_AFTER`, so returning near the
/// write cannot be the budget running out.
const BUDGET: Duration = Duration::from_secs(3);

/// The read end the stub reports as its readiness handle.
static READ_FD: AtomicI32 = AtomicI32::new(-1);
/// The kind `readiness_source` answers, so one stub serves both cases.
static KIND: AtomicU8 = AtomicU8::new(NROS_RMW_READINESS_WAITABLE as u8);
/// The largest timeout `drive_io` was asked to block for.
static MAX_DRIVE_MS: AtomicI32 = AtomicI32::new(-1);
/// The tests share the statics above; run them one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

unsafe extern "C" fn stub_open(
    _: *const core::ffi::c_char,
    _: u8,
    _: u32,
    _: *const core::ffi::c_char,
    _: *const NrosRmwSessionOptions,
    out: *mut NrosRmwSession,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<core::ffi::c_void>() };
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_close(_: *mut NrosRmwSession) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

// The entity slots the registry requires (issue 0332). Nothing here creates an
// entity; each refuses, as `nros-rmw-cffi`'s `ping_session` stubs do.
unsafe extern "C" fn stub_create_publisher(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *const nros_rmw_cffi::rmw_publisher_options_t,
    _: *mut NrosRmwPublisher,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_publisher(_: *mut NrosRmwPublisher) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_publish_raw(
    _: *const NrosRmwPublisher,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_subscription(
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
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take(
    _: *const NrosRmwSubscription,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_data(
    _: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_data = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_create_service(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwService,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_service(_: *mut NrosRmwService) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take_request(
    _: *const NrosRmwService,
    _: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    _: *mut i64,
    taken: *mut bool,
) -> NrosRmwRet {
    unsafe { *taken = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_request(
    _: *mut NrosRmwService,
    out_has_request: *mut bool,
) -> NrosRmwRet {
    // Phase 376 W3.d step A — flag out, status returned.
    unsafe { *out_has_request = false };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_send_response(
    _: *const NrosRmwService,
    _: i64,
    _: nros_rmw_cffi::generated::rmw_byte_span_t,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_create_client(
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_service_type_support_t,
    _: *const core::ffi::c_char,
    _: u32,
    _: *const NrosRmwQos,
    _: *mut NrosRmwClient,
) -> NrosRmwRet {
    NROS_RMW_RET_UNSUPPORTED
}
unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

/// Record the requested timeout, block for it when asked to (the
/// receive-blocking shape), and drain whatever arrived.
unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, timeout_ms: i32) -> NrosRmwRet {
    MAX_DRIVE_MS.fetch_max(timeout_ms, Ordering::SeqCst);
    if timeout_ms > 0 {
        std::thread::sleep(Duration::from_millis(timeout_ms as u64));
    }
    let fd = READ_FD.load(Ordering::SeqCst);
    let mut buf = [0u8; 64];
    // SAFETY: `fd` is the non-blocking read end the test owns for its run.
    while unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_readiness(
    _: *const rmw_session_t,
    out_kind: *mut u8,
    out_handle: *mut i32,
) -> NrosRmwRet {
    unsafe {
        *out_kind = KIND.load(Ordering::SeqCst);
        *out_handle = READ_FD.load(Ordering::SeqCst);
    }
    NROS_RMW_RET_OK
}

static VTABLE: NrosRmwVtable = NrosRmwVtable {
    create_session: Some(stub_open),
    destroy_session: Some(stub_close),
    drive_io: Some(stub_drive_io),
    create_publisher: Some(stub_create_publisher),
    destroy_publisher: Some(stub_destroy_publisher),
    publish: Some(stub_publish_raw),
    create_subscription: Some(stub_create_subscription),
    destroy_subscription: Some(stub_destroy_subscription),
    take: Some(stub_take),
    has_data: Some(stub_has_data),
    create_service: Some(stub_create_service),
    destroy_service: Some(stub_destroy_service),
    take_request: Some(stub_take_request),
    has_request: Some(stub_has_request),
    send_response: Some(stub_send_response),
    create_client: Some(stub_create_client),
    destroy_client: Some(stub_destroy_client),
    readiness_source: Some(stub_readiness),
    // No `set_wake_callback`: nothing but the handle can end the park.
    ..EMPTY_VTABLE
};

const STUB: &str = "readiness_park_stub";

fn open() -> Executor<'static> {
    let name = format!("{STUB}\0");
    // SAFETY: a NUL-terminated name and a vtable that lives for the process.
    let ret = unsafe { nros_rmw_cffi_register_named(name.as_ptr().cast(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK, "register the stub backend");
    let config = ExecutorConfig::new("stub/0")
        .node_name("readiness_park")
        .rmw(STUB);
    Executor::open(&config).expect("Executor::open over the stub")
}

/// One spin of `BUDGET`, with a byte written to the socket `WRITE_AFTER` in.
fn spin_across_a_write(kind: i32) -> (Duration, i32) {
    let (rx, mut tx) = UnixStream::pair().expect("socketpair");
    rx.set_nonblocking(true).expect("non-blocking read end");
    READ_FD.store(rx.as_raw_fd(), Ordering::SeqCst);
    KIND.store(kind as u8, Ordering::SeqCst);

    let mut executor = open();
    // Settle: the first spins register nothing, but let the executor reach its
    // steady state before the measured one.
    executor.spin_once(Duration::ZERO);
    MAX_DRIVE_MS.store(-1, Ordering::SeqCst);

    let writer = std::thread::spawn(move || {
        std::thread::sleep(WRITE_AFTER);
        tx.write_all(&[1]).expect("write");
        tx
    });
    let start = Instant::now();
    executor.spin_once(BUDGET);
    let elapsed = start.elapsed();
    let _tx = writer.join().expect("writer");
    let max_drive = MAX_DRIVE_MS.load(Ordering::SeqCst);
    drop(executor);
    READ_FD.store(-1, Ordering::SeqCst);
    (elapsed, max_drive)
}

#[test]
fn a_waitable_session_wakes_the_park_with_no_wake_callback() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (elapsed, max_drive) = spin_across_a_write(NROS_RMW_READINESS_WAITABLE);

    assert!(
        elapsed >= WRITE_AFTER,
        "the spin returned after {elapsed:?}, before the byte was written at {WRITE_AFTER:?}: \
         it did not wait at all, which is the busy loop RFC-0113 D2.1 forbids"
    );
    assert!(
        elapsed < WRITE_AFTER + Duration::from_millis(500),
        "the spin returned after {elapsed:?} for a byte written at {WRITE_AFTER:?}: the park \
         was not ended by the session's handle"
    );
    assert_eq!(
        max_drive, 0,
        "drive_io was asked to block for {max_drive} ms: a tier with a waitable session must \
         park on the handle and drain the transport non-blockingly (RFC-0113 D2.6)"
    );
}

#[test]
fn a_session_with_no_readiness_form_still_blocks_inside_drive_io() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (elapsed, max_drive) = spin_across_a_write(NROS_RMW_READINESS_NONE);

    // The receive-blocking wait phase-521 W6 retires: the whole budget is
    // handed to the transport. This is the control that makes the zero above
    // mean something.
    assert!(
        max_drive >= (BUDGET.as_millis() as i32) / 2,
        "drive_io was asked for at most {max_drive} ms, expected the spin's budget: the \
         control no longer exercises the receive-blocking wait"
    );
    assert!(elapsed >= BUDGET / 2, "{elapsed:?}");
}
