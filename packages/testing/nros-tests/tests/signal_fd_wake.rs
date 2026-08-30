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
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

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
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

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
