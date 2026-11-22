// Issue 2843 / phase-573 W3.
//! A C++ callback waiting for a service holds no executor lock while it
//! sleeps.
//!
//! Drives the C++ API (`nros_cpp_*`, the functions the C++ headers call)
//! over a stub cffi backend whose service is never visible, with the executor
//! lock a multi-level image installs. A timer callback calls
//! `nros_cpp_service_client_wait_for_service`, which inside a callback
//! sleep-polls until its deadline. While it waits, a second thread takes the
//! executor lock the way any other entry does. Before the fix the wait held
//! the lock for its whole timeout, so the second thread got in only after it;
//! now it gets in during the first sleep.
//!
//! No network and no router: the backend is a stub registered by name.

extern crate std;

use std::{
    boxed::Box,
    format,
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread_local,
    time::{Duration, Instant},
    vec,
};

use core::ffi::{c_char, c_void};
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_READINESS_NOTIFY, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService,
    NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named, rmw_session_t,
};

// ---------------------------------------------------------------- stub backend

unsafe extern "C" fn stub_open(
    _: *const c_char,
    _: u8,
    _: u32,
    _: *const c_char,
    _: *const NrosRmwSessionOptions,
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
    _: *const NrosRmwNode,
    _: *const nros_rmw_cffi::generated::rmw_message_type_support_t,
    _: *const c_char,
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
    _: *const c_char,
    _: u32,
    _: *const NrosRmwQos,
    out: *mut NrosRmwClient,
) -> NrosRmwRet {
    unsafe { (*out).backend_data = std::ptr::dangling_mut::<c_void>() };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_client(_: *mut NrosRmwClient) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
/// The server is never visible, so the wait runs to its deadline.
unsafe extern "C" fn stub_server_available(
    _: *const NrosRmwClient,
    out_available: *mut bool,
) -> NrosRmwRet {
    unsafe { *out_available = false };
    NROS_RMW_RET_OK
}
/// The stub honours what the test asks for: RELIABLE, VOLATILE, KEEP_LAST, depth.
unsafe extern "C" fn stub_qos(_: *const rmw_session_t, out_mask: *mut u32) -> NrosRmwRet {
    unsafe { *out_mask = 0b1_1011 };
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
    service_server_is_available: Some(stub_server_available),
    readiness_source: Some(stub_readiness),
    supported_qos_policies: Some(stub_qos),
    ..EMPTY_VTABLE
};

const STUB: &str = "cpp_wait_lock_stub";

// ------------------------------------------------------ the executor lock

/// A recursive lock with an owner, as the port's priority-inheriting mutex
/// is: the executor takes it for each entry and gives it for each callback's
/// window, and a callback that calls back in takes it again.
struct RecursiveLock {
    state: Mutex<(usize, usize)>, // (owner thread id, depth)
    free: Condvar,
}

static LOCK: RecursiveLock = RecursiveLock {
    state: Mutex::new((0, 0)),
    free: Condvar::new(),
};

fn this_thread() -> usize {
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    thread_local!(static ID: usize = NEXT.fetch_add(1, Ordering::SeqCst));
    ID.with(|id| *id)
}

unsafe extern "C" fn lock_take(_: *mut c_void) -> i8 {
    let me = this_thread();
    let mut s = LOCK.state.lock().unwrap();
    while s.1 > 0 && s.0 != me {
        s = LOCK.free.wait(s).unwrap();
    }
    s.0 = me;
    s.1 += 1;
    0
}

unsafe extern "C" fn lock_give(_: *mut c_void) -> i8 {
    let mut s = LOCK.state.lock().unwrap();
    assert!(
        s.1 > 0 && s.0 == this_thread(),
        "the lock is given by its holder"
    );
    s.1 -= 1;
    if s.1 == 0 {
        s.0 = 0;
        LOCK.free.notify_all();
    }
    0
}

unsafe extern "C" fn thread_id() -> usize {
    this_thread()
}

// ---------------------------------------------------------------- C++ API

use crate::{
    nros_cpp_executor_inner, nros_cpp_init_rmw, nros_cpp_node_create, nros_cpp_node_t,
    nros_cpp_qos_t, nros_cpp_spin_once,
    service::{nros_cpp_service_client_register, nros_cpp_service_client_wait_for_service},
    timer::nros_cpp_timer_create,
};

/// The per-image backend registration every C++ image links; the stub is
/// registered by name below instead. `native_runners` defines the same
/// symbol under `env`, so this one stands in only where that module is off.
/// cbindgen:ignore
#[cfg(not(all(feature = "rmw-cffi", feature = "env")))]
#[unsafe(no_mangle)]
extern "C" fn nros_app_register_backends() {}

unsafe extern "C" fn no_reply(_: *const u8, _: usize, _: *mut c_void) {}

/// How long the callback's wait runs, and how far into it the second thread
/// asks for the lock.
const WAIT_MS: u32 = 600;

static HANDLE: AtomicUsize = AtomicUsize::new(0);
static CLIENT: AtomicUsize = AtomicUsize::new(usize::MAX);
static WAITED: AtomicBool = AtomicBool::new(false);
static WAIT_RET: AtomicU64 = AtomicU64::new(u64::MAX);
static WAIT_STARTED_NS: AtomicU64 = AtomicU64::new(0);
static WAIT_ENDED_NS: AtomicU64 = AtomicU64::new(0);

fn now_ns(t0: Instant) -> u64 {
    t0.elapsed().as_nanos() as u64
}

static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// The timer callback: wait for a service that never appears.
unsafe extern "C" fn wait_in_callback(_: *mut c_void) {
    if WAITED.swap(true, Ordering::SeqCst) {
        return;
    }
    WAIT_STARTED_NS.store(now_ns(*T0.get().unwrap()), Ordering::SeqCst);
    let ret = unsafe {
        nros_cpp_service_client_wait_for_service(
            core::ptr::null_mut(),
            HANDLE.load(Ordering::SeqCst) as *mut c_void,
            CLIENT.load(Ordering::SeqCst),
            WAIT_MS,
        )
    };
    WAIT_ENDED_NS.store(now_ns(*T0.get().unwrap()), Ordering::SeqCst);
    WAIT_RET.store(ret as u32 as u64, Ordering::SeqCst);
}

#[test]
fn a_callback_waiting_for_a_service_leaves_the_executor_to_other_threads() {
    let t0 = *T0.get_or_init(Instant::now);
    let c_name = format!("{STUB}\0");
    // SAFETY: a NUL-terminated name and a vtable that lives for the process.
    let ret = unsafe { nros_rmw_cffi_register_named(c_name.as_ptr().cast(), &VTABLE) };
    assert_eq!(ret, NROS_RMW_RET_OK, "register the stub backend");

    let storage: &'static mut [u64] =
        Box::leak(vec![0u64; crate::CPP_EXECUTOR_OPAQUE_U64S].into_boxed_slice());
    let handle = storage.as_mut_ptr().cast::<c_void>();
    // SAFETY: NUL-terminated strings; `storage` is the documented size.
    let ret = unsafe {
        nros_cpp_init_rmw(
            c_name.as_ptr().cast(),
            c"stub/0".as_ptr(),
            0,
            c"cpp_wait_lock".as_ptr(),
            c"/".as_ptr(),
            handle,
        )
    };
    assert_eq!(
        ret,
        crate::NROS_CPP_RET_OK,
        "nros_cpp_init_rmw over the stub"
    );
    HANDLE.store(handle as usize, Ordering::SeqCst);

    // The lock a multi-level image installs, with the thread identity that
    // gives each callback its window.
    // SAFETY: a static lock that outlives the executor.
    let lock = unsafe {
        nros_node::executor::exec_lock::ExecLock::new(
            core::ptr::addr_of!(LOCK) as *mut c_void,
            lock_take,
            lock_give,
        )
        .expect("non-null lock context")
        .with_thread_id(thread_id)
    };
    // SAFETY: `nros_cpp_executor_inner` is the context's live executor, and
    // no second thread can reach it yet.
    unsafe {
        let exec = nros_cpp_executor_inner(handle) as *mut nros_node::executor::Executor<'static>;
        (*exec).set_exec_lock(Some(lock));
    }

    let mut node = core::mem::MaybeUninit::<nros_cpp_node_t>::zeroed();
    // SAFETY: as above.
    let ret = unsafe {
        nros_cpp_node_create(
            handle,
            c"cpp_wait_lock".as_ptr(),
            c"/".as_ptr(),
            node.as_mut_ptr(),
        )
    };
    assert_eq!(ret, 0, "node");
    let qos = nros_cpp_qos_t {
        reliability: crate::nros_cpp_qos_reliability_t::NROS_CPP_QOS_RELIABLE,
        durability: crate::nros_cpp_qos_durability_t::NROS_CPP_QOS_VOLATILE,
        history: crate::nros_cpp_qos_history_t::NROS_CPP_QOS_KEEP_LAST,
        liveliness_kind: crate::nros_cpp_qos_liveliness_t::NROS_CPP_QOS_LIVELINESS_NONE,
        depth: 10,
        deadline_ms: 0,
        lifespan_ms: 0,
        liveliness_lease_ms: 0,
        avoid_ros_namespace_conventions: 0,
        tx_express: 0,
    };
    let mut client = usize::MAX;
    // SAFETY: as above.
    let ret = unsafe {
        nros_cpp_service_client_register(
            node.as_ptr(),
            c"/never_served".as_ptr(),
            c"test_msgs/srv/Empty".as_ptr(),
            c"RIHS01_0000".as_ptr(),
            qos,
            no_reply,
            core::ptr::null_mut(),
            0,
            &mut client,
        )
    };
    assert_eq!(ret, 0, "client");
    CLIENT.store(client, Ordering::SeqCst);

    let mut timer = 0usize;
    // SAFETY: as above.
    let ret = unsafe {
        nros_cpp_timer_create(
            handle,
            1,
            Some(wait_in_callback),
            core::ptr::null_mut(),
            &mut timer,
        )
    };
    assert_eq!(ret, 0, "timer");

    // The spinning thread runs the timer, whose callback waits WAIT_MS.
    let spinner = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !WAITED.load(Ordering::SeqCst) && Instant::now() < deadline {
            // SAFETY: the context is live for the test.
            let _ = unsafe { nros_cpp_spin_once(HANDLE.load(Ordering::SeqCst) as *mut c_void, 5) };
        }
    });

    // Once the wait has begun, take the lock from here, as a sched task
    // sharing the executor would.
    let deadline = Instant::now() + Duration::from_secs(10);
    while WAIT_STARTED_NS.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < deadline, "the timer callback never ran");
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(50));
    let asked = now_ns(t0);
    // SAFETY: the static lock.
    unsafe { lock_take(core::ptr::null_mut()) };
    let got = now_ns(t0);
    unsafe { lock_give(core::ptr::null_mut()) };
    spinner.join().unwrap();

    let started = WAIT_STARTED_NS.load(Ordering::SeqCst);
    let ended = WAIT_ENDED_NS.load(Ordering::SeqCst);
    let waited_ms = (got - asked) / 1_000_000;
    // The wait really waited: it ran out its budget against a server that
    // never appears. A wait that returned at once (a failed lookup, an
    // unsupported probe) leaves the lock free and would pass the check below
    // on any code.
    assert_eq!(
        WAIT_RET.load(Ordering::SeqCst),
        crate::NROS_CPP_RET_TIMEOUT as u32 as u64,
        "the wait timed out"
    );
    assert!(
        ended - started >= u64::from(WAIT_MS) * 1_000_000,
        "the wait ran {} ms of its {WAIT_MS} ms budget",
        (ended - started) / 1_000_000
    );
    assert!(
        got < ended,
        "the second thread got the executor lock {waited_ms} ms after asking, at \
         {} ms; the callback's {WAIT_MS} ms wait ended at {} ms, so the wait \
         held the lock while it slept",
        got / 1_000_000,
        ended / 1_000_000
    );
}
