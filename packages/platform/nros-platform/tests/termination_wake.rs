//! phase-521 W6 (RFC-0113 D2) — a termination signal posts every live wake
//! object of the POSIX port.
//!
//! A hosted entry's spin parks until its next deadline. The termination guard
//! (issue 1732) only set a flag the loop read between spins, so SIGTERM would
//! have waited out that park. Now the guard's handler also posts each wake
//! object `nros_platform_wake_init` made, so a parked executor returns, sees
//! the flag and closes its session.
//!
//! Its own test binary: a delivered SIGTERM sets the process-wide flag.
#![cfg(feature = "platform-posix")]

use nros_platform::termination;

use core::ffi::c_void;
use std::{
    mem::MaybeUninit,
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn nros_platform_wake_init(w: *mut c_void) -> i8;
    fn nros_platform_wake_drop(w: *mut c_void) -> i8;
    fn nros_platform_wake_park_until_us(w: *mut c_void, deadline_us: u64) -> i8;
}

#[repr(align(16))]
struct WakeBuf([MaybeUninit<u8>; 256]);

#[test]
fn a_termination_signal_ends_a_long_park() {
    let mut buf = WakeBuf([MaybeUninit::uninit(); 256]);
    let w = buf.0.as_mut_ptr() as *mut c_void;
    // SAFETY: 256 aligned bytes, larger than the port's wake object.
    assert_eq!(unsafe { nros_platform_wake_init(w) }, 0);
    assert!(
        termination::install_guard() > 0,
        "the test runner already handles SIGTERM, so the guard cannot be tested here"
    );

    let killer = thread::spawn(|| {
        thread::sleep(Duration::from_millis(100));
        // SAFETY: deliver SIGTERM to this process; the guard turns the first
        // one into a flag.
        unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
    });
    let start = Instant::now();
    // SAFETY: `w` was initialised above.
    let rc = unsafe { nros_platform_wake_park_until_us(w, 5_000_000) };
    let elapsed = start.elapsed();
    killer.join().expect("killer");

    assert_eq!(
        rc, 0,
        "the park ended by timeout, not by the termination's post"
    );
    assert!(
        elapsed >= Duration::from_millis(100) && elapsed < Duration::from_secs(1),
        "a SIGTERM at 100 ms ended a 5 s park after {elapsed:?}"
    );
    assert!(termination::requested());
    // SAFETY: initialised above; nothing else uses it.
    assert_eq!(unsafe { nros_platform_wake_drop(w) }, 0);
}
