//! phase-521 W7 (RFC-0113 D2.5) — what one arrival for a LOW tier costs the
//! HIGH tiers that share its session.
//!
//! Several tiers over one session share the backend's single wake slot through
//! the executor's wake fan-out (RFC-0106 D6). A backend that ATTRIBUTES an
//! arrival names its entity (`subscription_set_on_new_message_callback`, the
//! entity waker, phase-397), so only the tier that owns the entity wakes. A
//! backend that cannot attribute fires the session wake, and every tier on the
//! session wakes, finds nothing of its own, and parks again: the fan-out D2.5
//! accepts as "a known cost, documented per backend". This file states that
//! cost as a measured bound instead of a sentence.
//!
//! In-tree, the non-attributing backend is uORB (all three entity slots NULL),
//! which runs only inside PX4; zenoh, XRCE and Cyclone fill the slots. So the
//! measurement runs on Linux over a stub backend registered under its own name,
//! in two shapes that differ ONLY in that slot:
//!
//! - NON-ATTRIBUTING: the slot is NULL and an arrival calls the session wake.
//!   One arrival for the low tier wakes every high tier (the fan-out count is
//!   `N - 1`), and each such wake costs its tier a bounded slice of CPU.
//! - ATTRIBUTING: the slot stores the entity's callback, and an arrival calls
//!   that instead. One arrival for the low tier wakes no high tier.
//!
//! "Woken" is measured, not inferred: after one arrival, each high tier is
//! spun with a long budget, and a tier whose spin returns well inside it was
//! woken by that arrival (its executor's wake flag was set). A tier that was
//! not woken parks for the whole budget.
//!
//! Run: `cargo nextest run -p nros-tests --features trigger-test --test sched_task_wake_fanout`.

#![cfg(all(feature = "trigger-test", unix))]
// The executors are boxed on purpose: the low tier owns the session every
// other tier's handle refers to, so it must not move when the `Vec` grows.
#![allow(clippy::vec_box)]

// The POSIX platform port (the wake object and its park) is force-linked by
// `nros_tests`'s own `lib.rs`; `as _` keeps that reference without a name.
use nros_tests as _;

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};

use nros_node::{QoSProfile, executor::*};
use nros_rmw_cffi::{
    EMPTY_VTABLE, NROS_RMW_READINESS_NOTIFY, NROS_RMW_RET_OK, NROS_RMW_RET_UNSUPPORTED,
    NrosRmwClient, NrosRmwNode, NrosRmwPublisher, NrosRmwQos, NrosRmwRet, NrosRmwService,
    NrosRmwSession, NrosRmwSessionOptions, NrosRmwSubscription, NrosRmwVtable,
    nros_rmw_cffi_register_named, rmw_session_t,
};

/// Tiers over the one session: one low tier that owns the arrivals, the rest
/// high tiers that own nothing the arrivals touch.
const TIERS: usize = 4;
/// Arrivals in the cost run.
const ARRIVALS: usize = 200;
/// The spin budget a tier that was NOT woken waits out. A woken tier returns
/// in microseconds, so anything under a quarter of this is a wake.
const PROBE: Duration = Duration::from_millis(200);
const TYPE: &str = "std_msgs/msg/Empty";
/// A CDR-encapsulated `std_msgs/Empty` (the 4-byte header and one pad byte).
const PAYLOAD: &[u8] = &[0, 1, 0, 0, 0];

/// The session wake the runtime installed (the fan-out's callback).
static WAKE_CB: AtomicUsize = AtomicUsize::new(0);
static WAKE_CTX: AtomicUsize = AtomicUsize::new(0);

/// One subscription the stub created: its pending sample and, when the shape
/// attributes, the entity callback the runtime registered on it.
struct Sub {
    pending: AtomicBool,
    cb: AtomicUsize,
    user: AtomicUsize,
}
static SUBS: [Sub; TIERS] = [const {
    Sub {
        pending: AtomicBool::new(false),
        cb: AtomicUsize::new(0),
        user: AtomicUsize::new(0),
    }
}; TIERS];
static NEXT_SUB: AtomicUsize = AtomicUsize::new(0);
/// The tests share the statics above; run them one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn sub_index(sub: *const NrosRmwSubscription) -> usize {
    // SAFETY: the runtime hands back the struct the stub filled.
    (unsafe { (*sub).backend_data } as usize) - 1
}

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
/// Nothing to drain: arrivals are the `pending` flags.
unsafe extern "C" fn stub_drive_io(_: *mut NrosRmwSession, _: i32) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
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
    out: *mut NrosRmwSubscription,
) -> NrosRmwRet {
    let idx = NEXT_SUB.fetch_add(1, SeqCst);
    assert!(idx < TIERS, "the stub has room for {TIERS} subscriptions");
    // SAFETY: `out` is the runtime's struct for this subscription.
    unsafe { (*out).backend_data = (idx + 1) as *mut core::ffi::c_void };
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_destroy_subscription(_: *mut NrosRmwSubscription) -> NrosRmwRet {
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_take(
    sub: *const NrosRmwSubscription,
    out: *mut nros_rmw_cffi::generated::rmw_mut_byte_span_t,
    taken: *mut bool,
) -> NrosRmwRet {
    let s = &SUBS[sub_index(sub)];
    // SAFETY: the runtime's out-parameters, valid for the call.
    unsafe {
        if s.pending.swap(false, SeqCst) && (*out).capacity >= PAYLOAD.len() {
            core::ptr::copy_nonoverlapping(PAYLOAD.as_ptr(), (*out).data, PAYLOAD.len());
            (*out).len = PAYLOAD.len();
            *taken = true;
        } else {
            *taken = false;
        }
    }
    NROS_RMW_RET_OK
}
unsafe extern "C" fn stub_has_data(
    sub: *mut NrosRmwSubscription,
    out_has_data: *mut bool,
) -> NrosRmwRet {
    // SAFETY: the runtime's out-parameter.
    unsafe { *out_has_data = SUBS[sub_index(sub)].pending.load(SeqCst) };
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
unsafe extern "C" fn stub_has_request(_: *mut NrosRmwService, out: *mut bool) -> NrosRmwRet {
    unsafe { *out = false };
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
/// Both shapes wake through a callback: RFC-0113 D4's `Notify`.
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
    ctx: *mut core::ffi::c_void,
) -> NrosRmwRet {
    WAKE_CB.store(cb.map_or(0, |f| f as usize), SeqCst);
    WAKE_CTX.store(ctx as usize, SeqCst);
    NROS_RMW_RET_OK
}
/// The attributing shape's entity slot: keep the callback against the entity.
unsafe extern "C" fn stub_set_on_new_message(
    sub: *mut NrosRmwSubscription,
    cb: nros_rmw_cffi::generated::rmw_event_callback_t,
    user: *const core::ffi::c_void,
) -> NrosRmwRet {
    let s = &SUBS[sub_index(sub)];
    s.cb.store(cb.map_or(0, |f| f as usize), SeqCst);
    s.user.store(user as usize, SeqCst);
    NROS_RMW_RET_OK
}

/// The non-attributing shape: no entity slot, so every arrival is a session
/// wake (uORB's vtable).
static FANOUT_VTABLE: NrosRmwVtable = NrosRmwVtable {
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

/// The attributing shape: the same stub with the entity slot filled.
static ATTRIBUTING_VTABLE: NrosRmwVtable = NrosRmwVtable {
    subscription_set_on_new_message_callback: Some(stub_set_on_new_message),
    ..FANOUT_VTABLE
};

/// An arrival for subscription `idx`: its sample becomes takeable, and the
/// backend wakes whom it can name — the entity when its callback is
/// registered, otherwise the session.
fn arrive(idx: usize) {
    let s = &SUBS[idx];
    s.pending.store(true, SeqCst);
    let cb = s.cb.load(SeqCst);
    if cb != 0 {
        // SAFETY: stored from an `rmw_event_callback_t` by the entity slot;
        // the executor that registered it is alive for the test.
        let cb: unsafe extern "C" fn(*const core::ffi::c_void, usize) =
            unsafe { std::mem::transmute(cb) };
        unsafe { cb(s.user.load(SeqCst) as *const core::ffi::c_void, 1) };
        return;
    }
    let cb = WAKE_CB.load(SeqCst);
    assert_ne!(cb, 0, "the runtime installed no session wake callback");
    // SAFETY: stored from an `Option<unsafe extern "C" fn>` by the wake slot.
    let cb: unsafe extern "C" fn(*mut core::ffi::c_void) = unsafe { std::mem::transmute(cb) };
    unsafe { cb(WAKE_CTX.load(SeqCst) as *mut core::ffi::c_void) };
}

/// This thread's CPU time, from `CLOCK_THREAD_CPUTIME_ID` (nanosecond
/// resolution; `getrusage`'s thread times are tick-sampled on this kernel and
/// read zero for a sub-microsecond pass).
fn thread_cpu() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes the struct it is handed and nothing else.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// `TIERS` executors over one session of the stub registered as `name`: the
/// first (the low tier) opens it, the rest join it. Each has one subscription,
/// and the low tier's is subscription 0.
fn open_sched_tasks(name: &str, vtable: &'static NrosRmwVtable) -> Vec<Box<Executor<'static>>> {
    for s in &SUBS {
        s.pending.store(false, SeqCst);
        s.cb.store(0, SeqCst);
        s.user.store(0, SeqCst);
    }
    NEXT_SUB.store(0, SeqCst);
    WAKE_CB.store(0, SeqCst);
    let c_name = format!("{name}\0");
    // SAFETY: a NUL-terminated name and a vtable that lives for the process.
    let ret = unsafe { nros_rmw_cffi_register_named(c_name.as_ptr().cast(), vtable) };
    assert_eq!(ret, NROS_RMW_RET_OK, "register the stub backend");
    let config = ExecutorConfig::new("stub/0")
        .node_name("sched_task_wake_fanout")
        .rmw(name);
    // Boxed: the session a handle refers to must not move, and a `Vec` of
    // executors moves its elements when it grows.
    let low = Box::new(Executor::open(&config).expect("Executor::open over the stub"));
    let mut tiers = vec![low];
    for _ in 1..TIERS {
        let handle = tiers[0].session_handle();
        // SAFETY: the low tier opened the session, is boxed (so it does not
        // move), and is dropped LAST (first in the vector; `drop_sched_tasks` drops
        // in reverse).
        tiers.push(Box::new(unsafe {
            Executor::open_with_session_handle(handle)
        }));
    }
    for (i, exec) in tiers.iter_mut().enumerate() {
        let node = exec
            .node_builder(&format!("tier{i}"))
            .build()
            .expect("node");
        exec.node_mut(node)
            .subscription(&format!("/tier{i}"))
            .generic(TYPE, "")
            .qos(QoSProfile::default())
            .rx_buffer::<64>()
            .build(|_data: &[u8]| {})
            .expect("register subscription");
    }
    // Settle every tier, so no wake left over from setup is counted.
    for exec in tiers.iter_mut() {
        exec.spin_once(Duration::ZERO);
    }
    tiers
}

fn drop_sched_tasks(mut tiers: Vec<Box<Executor<'static>>>) {
    while let Some(exec) = tiers.pop() {
        drop(exec);
    }
}

/// How many high tiers one arrival for the low tier woke. Each high tier is
/// spun once with [`PROBE`]: a woken tier's flag is already set, so its spin
/// returns at once; any other parks for the whole budget.
fn high_sched_tasks_woken_by_one_low_arrival(tiers: &mut [Box<Executor<'static>>]) -> usize {
    arrive(0);
    // The low tier takes its sample, as its own thread would.
    tiers[0].spin_once(Duration::ZERO);
    let mut woken = 0;
    for exec in tiers[1..].iter_mut() {
        let start = Instant::now();
        exec.spin_once(PROBE);
        if start.elapsed() < PROBE / 4 {
            woken += 1;
        }
    }
    woken
}

#[test]
fn a_non_attributing_backend_wakes_every_high_sched_task_and_each_wake_is_bounded() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut tiers = open_sched_tasks("sched_task_fanout_stub", &FANOUT_VTABLE);

    let woken = high_sched_tasks_woken_by_one_low_arrival(&mut tiers);
    assert_eq!(
        woken,
        TIERS - 1,
        "one arrival for the low tier woke {woken} of {} high tiers over a session that cannot \
         attribute: the fan-out is every member of the session (RFC-0106 D6)",
        TIERS - 1
    );

    // The cost, measured as a deployment pays it: every tier on its own
    // thread, parked in a long `spin_once`, as a tier task is. ARRIVALS
    // arrivals for the low tier, 2 ms apart; each high tier counts how often
    // its park ended and what its thread spent in CPU over the run, which
    // includes the wake itself (the futex, the switch-in), not only the pass.
    let (spurious, per_wake, per_arrival) = threaded_fanout_cost(tiers);
    eprintln!(
        "fan-out over {TIERS} tiers, one thread each: {per_arrival:.2} high-tier wakes per \
         low-tier arrival ({spurious} over {ARRIVALS} arrivals), {per_wake:?} of thread CPU each"
    );
    assert!(
        per_arrival >= (TIERS - 1) as f64 * 0.9,
        "{per_arrival:.2} high-tier wakes per arrival over a non-attributing session: expected \
         {} (every member), so the threaded run is not measuring the fan-out",
        TIERS - 1
    );
    // The stated bound (RFC-0113 D2.5): a spurious wake is one thread wake and
    // one non-blocking pass, never a wait. 100 us of CPU is an order of
    // magnitude above what it costs here, and a tenth of a 1 kHz tier's period.
    assert!(
        per_wake < Duration::from_micros(100),
        "a spurious fan-out wake cost {per_wake:?} of CPU: more than a wake and one pass"
    );
}

/// A tier executor moved to its own thread. The executor is only ever used by
/// that thread, and joined before the session owner drops.
struct SendExec(Box<Executor<'static>>);
// SAFETY: see the type's comment; the session's wake object and fan-out are
// the cross-thread parts, and they are built for exactly this (tier tasks).
unsafe impl Send for SendExec {}

/// Run each tier on a thread, fire ARRIVALS low-tier arrivals, and return
/// (high-tier wakes, CPU per high-tier wake, high-tier wakes per arrival).
fn threaded_fanout_cost(tiers: Vec<Box<Executor<'static>>>) -> (u32, Duration, f64) {
    use std::sync::{Arc, Barrier};
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(Barrier::new(TIERS + 1));
    let mut handles = Vec::new();
    // Spawn the members first and the owner last, so the owner (index 0) is
    // joined and dropped last.
    let mut tiers = tiers;
    let mut owned: Vec<(usize, SendExec)> = Vec::new();
    while let Some(exec) = tiers.pop() {
        owned.push((tiers.len(), SendExec(exec)));
    }
    for (idx, exec) in owned {
        let stop = stop.clone();
        let ready = ready.clone();
        handles.push((
            idx,
            std::thread::spawn(move || {
                let mut exec = exec;
                ready.wait();
                let c0 = thread_cpu();
                let mut wakes = 0u32;
                while !stop.load(SeqCst) {
                    exec.0.spin_once(Duration::from_secs(1));
                    wakes += 1;
                }
                (wakes, thread_cpu() - c0, exec)
            }),
        ));
    }
    ready.wait();
    // Let every tier reach its park before the first arrival.
    std::thread::sleep(Duration::from_millis(50));
    for _ in 0..ARRIVALS {
        arrive(0);
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(20));
    stop.store(true, SeqCst);
    // The stop flag is read after a park; end every park now.
    let cb = WAKE_CB.load(SeqCst);
    // SAFETY: as in `arrive`.
    let cb: unsafe extern "C" fn(*mut core::ffi::c_void) = unsafe { std::mem::transmute(cb) };
    unsafe { cb(WAKE_CTX.load(SeqCst) as *mut core::ffi::c_void) };

    let mut high_wakes = 0u32;
    let mut high_cpu = Duration::ZERO;
    let mut back: Vec<(usize, SendExec)> = Vec::new();
    for (idx, h) in handles {
        let (wakes, cpu, exec) = h.join().expect("tier thread");
        if idx != 0 {
            // The last wake is the stop; it is not an arrival's.
            high_wakes += wakes.saturating_sub(1);
            high_cpu += cpu;
        }
        back.push((idx, exec));
    }
    // Drop members before the owner.
    back.sort_by_key(|(idx, _)| core::cmp::Reverse(*idx));
    for (_, exec) in back {
        drop(exec);
    }
    let per_wake = high_cpu / high_wakes.max(1);
    (high_wakes, per_wake, high_wakes as f64 / ARRIVALS as f64)
}

#[test]
fn an_attributing_backend_does_not_wake_a_high_sched_task_for_a_low_sched_task_arrival() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut tiers = open_sched_tasks("sched_task_attributing_stub", &ATTRIBUTING_VTABLE);
    // Control: every subscription took the entity callback, so this run is
    // about attribution and not a backend that silently fell back.
    for (i, s) in SUBS.iter().enumerate() {
        assert_ne!(
            s.cb.load(SeqCst),
            0,
            "subscription {i} registered no entity callback"
        );
    }

    let woken = high_sched_tasks_woken_by_one_low_arrival(&mut tiers);
    assert_eq!(
        woken, 0,
        "one arrival for the low tier woke {woken} high tiers on a backend that names the \
         arrival's entity: only the owning tier may wake (RFC-0113 D2.5)"
    );
    // Control: the arrival really reached the low tier.
    arrive(0);
    let start = Instant::now();
    tiers[0].spin_once(PROBE);
    assert!(
        start.elapsed() < PROBE / 4,
        "the low tier was not woken by its own arrival, so the high tiers sleeping proves nothing"
    );
    drop_sched_tasks(tiers);
}
