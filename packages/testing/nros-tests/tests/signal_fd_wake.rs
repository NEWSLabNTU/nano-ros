//! LINUX signalfd worker — a `write(2)` to the executor's eventfd unblocks a
//! `spin_once` that is parked on the wake condvar. `signalfd`/`eventfd` are
//! Linux syscalls, not POSIX — hence the `target_os = "linux"` gate below.
//!
//! `write(2)` on an eventfd is on the async-signal-safe list per `eventfd(2)`,
//! so this is the same operation a real signal handler performs; the second
//! test installs an actual `SIGUSR1` handler and does it from inside one.
//!
//! # Why this lives in `nros-tests` and not in `nros-node` (issue 0612)
//!
//! It used to be `packages/core/nros-node/tests/signal_fd_wake.rs`, where it
//! could not pass by construction. `NodeWake` — the code under test — is gated
//! `all(feature = "alloc", feature = "rmw-cffi")`, and `nros-node` registers no
//! cffi backend of its own, so `Executor::open` had nothing to open against and
//! both tests took an `eprintln!("[SKIPPED]") + return` branch that reports
//! PASS. Turning the gate the other way removes `rmw-cffi` and with it the
//! whole wake path. No invocation both compiled the code under test and gave it
//! a session.
//!
//! A test crate that already owns a router fixture and a registered backend is
//! the missing half, and this is it: `zenohd_unique` supplies the session, the
//! force-link below supplies the backend, and `nros-tests` has lanes. Nothing
//! about the wake path is loosened to get here — the `#![cfg]` is narrower than
//! before, not wider.
//!
//! And the opposite direction (issue 2144, RFC-0112 D2.2): `ready_fd()` is a
//! READABLE eventfd that a host loop (GLib, Qt, epoll) watches to learn that
//! work arrived, so it spins only then.
//!
//! Run: `just check required-features-tests` (or `cargo nextest run -p
//! nros-tests --features signal-fd-wake-test --test signal_fd_wake`).

#![cfg(all(feature = "signal-fd-wake-test", target_os = "linux"))]

// Force-link the zenoh-pico backend so its `.init_array` ctor registers the
// vtable before `Executor::open` runs; without it the cffi registry is empty
// and the resolver returns `NoBackend`. The platform C port that defines
// `nros_platform_wake_*` is force-linked by `nros_tests`'s own `lib.rs`, which
// this file pulls in for the fixture.
use nros_rmw_zenoh as _;

use std::time::{Duration, Instant};

use nros_node::executor::*;
use nros_tests::fixtures::{ZenohRouter, require_zenohd, zenohd_unique};
use rstest::rstest;

/// The wake must arrive after the trigger, not merely soon. Asserting only an
/// upper bound (which is what this test shipped with) passes a `spin_once` that
/// never waited at all, which is indistinguishable from the bug.
const TRIGGER_DELAY_MS: u64 = 30;
/// Slack above `TRIGGER_DELAY_MS` for the cv wake itself plus scheduling.
const WAKE_SLACK_MS: u64 = 100;

fn open_executor(locator: &str, node_name: &str, domain_id: u32) -> Executor<'static> {
    let config = ExecutorConfig::new(locator)
        .node_name(node_name)
        .domain_id(domain_id);
    Executor::open(&config).expect("Executor::open failed")
}

fn assert_woke_on_trigger(elapsed: Duration, what: &str) {
    assert!(
        elapsed >= Duration::from_millis(TRIGGER_DELAY_MS),
        "spin_once returned after {elapsed:?}, before the {what} at +{TRIGGER_DELAY_MS} ms — \
         it did not block, so this run proves nothing about the wake path"
    );
    assert!(
        elapsed < Duration::from_millis(TRIGGER_DELAY_MS + WAKE_SLACK_MS),
        "spin_once took {elapsed:?} — expected the {what} at +{TRIGGER_DELAY_MS} ms to \
         unblock it within {WAKE_SLACK_MS} ms, so the cv wake is not firing"
    );
}

/// A cross-thread `write(eventfd, 1)` unblocks a parked `spin_once`.
#[rstest]
fn eventfd_write_unblocks_spin_once(zenohd_unique: ZenohRouter) {
    require_zenohd();

    let mut executor = open_executor(&zenohd_unique.locator(), "signal_fd_wake_test", 94);

    let fd = executor.signal_fd().expect("signal_fd() failed");
    assert!(fd >= 0, "signal_fd must return non-negative fd");

    let trigger_thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(TRIGGER_DELAY_MS));
        let one: u64 = 1;
        // SAFETY: `write(2)` on an eventfd is async-signal-safe; eventfd
        // semantics require exactly an 8-byte buffer.
        let n = unsafe { libc::write(fd, &one as *const u64 as *const core::ffi::c_void, 8) };
        assert!(n == 8, "eventfd write must be 8 bytes; got {n}");
    });

    let start = Instant::now();
    executor.spin_once(Duration::from_millis(1000));
    let elapsed = start.elapsed();

    trigger_thread.join().unwrap();
    assert_woke_on_trigger(elapsed, "eventfd write");
}

// Static fd for the SIGUSR1 handler — set before the `sigaction` install, read
// inside the (async-signal-safe) handler.
static SIGNAL_FD_FOR_HANDLER: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn sigusr1_wake_handler(_sig: core::ffi::c_int) {
    // SAFETY: `write(2)` on an eventfd is on the POSIX async-signal-safe list
    // per `eventfd(2)`, and an atomic load is likewise safe in a handler.
    let fd = SIGNAL_FD_FOR_HANDLER.load(std::sync::atomic::Ordering::SeqCst);
    if fd >= 0 {
        let one: u64 = 1;
        unsafe {
            libc::write(fd, &one as *const u64 as *const core::ffi::c_void, 8);
        }
    }
}

/// A real `SIGUSR1` handler writing the signalfd unblocks a parked `spin_once`.
#[rstest]
fn sigusr1_handler_wakes_spin_once(zenohd_unique: ZenohRouter) {
    require_zenohd();

    let mut executor = open_executor(&zenohd_unique.locator(), "sigusr1_test", 93);

    let fd = executor.signal_fd().expect("signal_fd() failed");
    SIGNAL_FD_FOR_HANDLER.store(fd, std::sync::atomic::Ordering::SeqCst);

    // SAFETY: installing a handler for SIGUSR1, whose default disposition is
    // restored below before this test returns.
    unsafe {
        let mut sa: libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = sigusr1_wake_handler as *const () as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        let rc = libc::sigaction(libc::SIGUSR1, &sa, core::ptr::null_mut());
        assert_eq!(rc, 0, "sigaction failed");
    }

    let pid = unsafe { libc::getpid() };
    let killer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(TRIGGER_DELAY_MS));
        unsafe { libc::kill(pid, libc::SIGUSR1) };
    });

    let start = Instant::now();
    executor.spin_once(Duration::from_millis(1000));
    let elapsed = start.elapsed();

    killer.join().unwrap();

    // Restore the default disposition so a later SIGUSR1 does not run a handler
    // holding a dangling fd.
    // SAFETY: same contract as the install above.
    unsafe {
        let mut sa: libc::sigaction = core::mem::zeroed();
        sa.sa_sigaction = libc::SIG_DFL;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGUSR1, &sa, core::ptr::null_mut());
    }
    SIGNAL_FD_FOR_HANDLER.store(-1, std::sync::atomic::Ordering::SeqCst);

    assert_woke_on_trigger(elapsed, "SIGUSR1");
}

/// `poll(2)` `fd` for `POLLIN` for up to `ms`.
fn readable_within(fd: core::ffi::c_int, ms: i32) -> bool {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one valid pollfd.
    let n = unsafe { libc::poll(&mut p, 1, ms) };
    n == 1 && p.revents & libc::POLLIN != 0
}

/// Read (and so re-arm) the eventfd; `true` when it held a count.
fn drain(fd: core::ffi::c_int) -> bool {
    let mut buf = [0u8; 8];
    // SAFETY: an 8-byte read from an eventfd into an 8-byte buffer.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut core::ffi::c_void, 8) };
    n == 8 && u64::from_ne_bytes(buf) > 0
}

/// Issue 2144 -- a host watching `ready_fd()` sees it become readable for an
/// arrival with nothing spinning, re-arms it by reading, and the spin it then
/// runs dispatches the work.
#[rstest]
fn ready_fd_becomes_readable_on_arrival(zenohd_unique: ZenohRouter) {
    require_zenohd();

    let mut executor = open_executor(&zenohd_unique.locator(), "ready_fd_test", 92);
    let node = executor
        .node_builder("ready_fd_test")
        .build()
        .expect("node");
    let received = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let received_cb = received.clone();
    executor
        .node_mut(node)
        .subscription("/ready_fd/data")
        .generic("std_msgs/msg/Empty", "")
        .qos(nros_node::QoSProfile::default())
        .rx_buffer::<256>()
        .build(move |_data: &[u8]| {
            received_cb.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .expect("subscription");
    let publisher = executor
        .node_mut(node)
        .create_publisher_raw("/ready_fd/data", "std_msgs/msg/Empty", "")
        .expect("publisher");
    let (_, guard) = executor
        .node_mut(node)
        .create_guard_condition(|| {})
        .expect("guard condition");

    let fd = executor.ready_fd().expect("ready_fd()");
    assert!(fd >= 0);
    assert_eq!(
        executor.ready_fd().expect("ready_fd()"),
        fd,
        "one fd per executor"
    );
    let wake_fd = executor.signal_fd().expect("signal_fd()");
    assert_ne!(
        fd, wake_fd,
        "the readiness fd is not the signal handlers' write end"
    );

    // A trigger wakes synchronously, so the fd is readable at once, and a read
    // takes the count.
    guard.trigger();
    assert!(
        readable_within(fd, 0),
        "a guard trigger did not make ready_fd readable"
    );
    assert!(drain(fd), "ready_fd was readable but held no count");
    executor.spin_some(Duration::ZERO).expect("spin_some");

    // An arrival, with nothing spinning.
    let start = Instant::now();
    while received.load(std::sync::atomic::Ordering::SeqCst) == 0 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "no arrival was dispatched within 5 s of ready_fd turning readable"
        );
        publisher
            .publish_serialized(&[0, 1, 0, 0])
            .expect("publish");
        assert!(
            readable_within(fd, 5000),
            "ready_fd did not become readable within 5 s of a publish, with nothing spinning"
        );
        drain(fd);
        executor.spin_some(Duration::ZERO).expect("spin_some");
    }
}
