//! Issue 2727 -- a guard condition handle outlives its executor soundly,
//! through a REAL `Executor::drop` over a backend that takes the wake
//! callback.
//!
//! The handle held a raw pointer to the executor's `WakeCtx`, whose only `Arc`
//! `Executor::drop` frees, so `trigger` after the drop called the wake callback
//! on freed memory, from safe code. It now owns an `Arc` clone. Asserted:
//!
//! 1. Dropping the executor clears the backend's wake callback (the backend
//!    holds no pointer past the executor).
//! 2. The handle is then the wake state's only owner: it kept it alive.
//! 3. `trigger` afterwards sets the flag and reaches that live state (under
//!    AddressSanitizer this is the read that used to be a use-after-free).
//!
//! Run: `cargo nextest run -p nros-tests --features trigger-test --test guard_condition_outlives_executor`.

#![cfg(all(feature = "trigger-test", unix))]

// The POSIX platform port (the wake object) is force-linked by `nros_tests`'s
// own `lib.rs`; `as _` keeps that reference without a name.
use nros_tests as _;

use std::sync::atomic::{AtomicUsize, Ordering};

use nros_node::executor::*;
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_READINESS_NOTIFY, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService,
    NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named, rmw_session_t,
};

/// The wake callback the runtime installed (0 = none), as a backend stores it.
static WAKE_CB: AtomicUsize = AtomicUsize::new(0);

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

unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_readiness(
    _: *const rmw_session_t,
    out_kind: *mut u8,
    out_handle: *mut i32,
) -> NrosRmwRet {
    unsafe {
        *out_kind = NROS_RMW_READINESS_NOTIFY as u8;
        *out_handle = -1;
    }
    NROS_RMW_RET_OK
}

unsafe extern "C" fn stub_set_wake_callback(
    _: *mut NrosRmwSession,
    cb: Option<unsafe extern "C" fn(*mut core::ffi::c_void)>,
    _ctx: *mut core::ffi::c_void,
) -> NrosRmwRet {
    WAKE_CB.store(cb.map_or(0, |f| f as usize), Ordering::SeqCst);
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
    set_wake_callback: Some(stub_set_wake_callback),
    ..EMPTY_VTABLE
};

const STUB: &str = "gc_outlives_stub";

#[test]
fn a_guard_condition_triggered_after_its_executor_is_dropped_reaches_live_state() {
    let c_name = format!("{STUB}\0");
    // SAFETY: a NUL-terminated name and a vtable that lives for the process.
    let ret = unsafe { nros_rmw_cffi_register_named(c_name.as_ptr().cast(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK, "register the stub backend");
    let config = ExecutorConfig::new("stub/0")
        .node_name("guard_condition_outlives_executor")
        .rmw(STUB);
    let mut executor = Executor::open(&config).expect("Executor::open over the stub");
    let (_id, guard) = executor
        .register_guard_condition(|| {})
        .expect("register_guard_condition");
    let before = guard.wake_state_owners();
    assert!(
        before >= 2,
        "the executor and the handle both own the wake state, saw {before}"
    );

    drop(executor);
    assert_eq!(
        WAKE_CB.load(Ordering::SeqCst),
        0,
        "Executor::drop clears the backend's wake callback"
    );
    assert_eq!(
        guard.wake_state_owners(),
        1,
        "the handle alone keeps the wake state alive past its executor"
    );

    guard.trigger();
    assert!(guard.is_triggered(), "the trigger set the handle's flag");
    let second = guard.clone();
    assert_eq!(second.wake_state_owners(), 2, "a clone owns it too");
}
