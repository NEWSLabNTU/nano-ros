//! Phase 130 — exercise the POSIX C-port `nros_platform_wake_*`
//! binary-semaphore primitive. Verifies init/drop, wait_ms timeout
//! semantics, signal coalescing, and cross-thread signal delivery.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-platform-cffi --features posix-c-port --test c_port_posix_wake
//! ```

#![cfg(feature = "posix-c-port")]

// Force-link the crate's rlib so its build.rs `cargo:rustc-link-lib`
// directives (including `static=nros_platform_posix`) reach the test
// binary's link line. Without this, the test only references raw FFI
// symbols and the gnu-ld static-archive search skips the platform
// member.
use nros_platform_cffi as _;

use core::ffi::c_void;
use std::{
    mem::MaybeUninit,
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn nros_platform_wake_init(w: *mut c_void) -> i8;
    fn nros_platform_wake_drop(w: *mut c_void) -> i8;
    fn nros_platform_wake_wait_ms(w: *mut c_void, timeout_ms: u32) -> i8;
    fn nros_platform_wake_signal(w: *mut c_void) -> i8;
    fn nros_platform_wake_signal_from_isr(w: *mut c_void) -> i8;
    fn nros_platform_wake_park_until_us(w: *mut c_void, deadline_us: u64) -> i8;
    fn nros_platform_wake_park_granularity_us() -> u64;
    fn nros_platform_wake_storage_size() -> usize;
    fn nros_platform_wake_storage_align() -> usize;
}

// Caller allocates aligned storage sized at runtime. Use a generous
// fixed-size MaybeUninit buffer that covers `sem_t` on every POSIX
// target we care about (sem_t is ~32 bytes on Linux x86_64, ~16 on
// macOS pthread_cond_t+mutex+flag layout). 256 bytes is overkill on
// purpose so the test doesn't have to allocate. The runtime probe
// (`storage_size()`) is asserted below.
#[repr(align(16))]
struct WakeBuf([MaybeUninit<u8>; 256]);

impl WakeBuf {
    fn new() -> Self {
        WakeBuf([MaybeUninit::uninit(); 256])
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.0.as_mut_ptr() as *mut c_void
    }
}

#[test]
fn storage_probe_fits_in_buffer() {
    let size = unsafe { nros_platform_wake_storage_size() };
    let align = unsafe { nros_platform_wake_storage_align() };
    assert!(size > 0, "storage size must be non-zero");
    assert!(size <= 256, "WakeBuf too small: need {} bytes", size);
    assert!(align <= 16, "WakeBuf alignment too weak: need {}", align);
}

#[test]
fn init_drop_roundtrip() {
    let mut buf = WakeBuf::new();
    assert_eq!(unsafe { nros_platform_wake_init(buf.as_mut_ptr()) }, 0);
    assert_eq!(unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) }, 0);
}

#[test]
fn wait_returns_timeout_when_no_signal() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) };
    let elapsed = t0.elapsed();

    assert_eq!(rc, 1, "expected timeout (rc=1), got {}", rc);
    assert!(
        elapsed >= Duration::from_millis(45),
        "wait returned too fast: {:?}",
        elapsed
    );
    // Upper bound: scheduling jitter. Be generous on CI.
    assert!(
        elapsed < Duration::from_millis(500),
        "wait took unexpectedly long: {:?}",
        elapsed
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

#[test]
fn signal_before_wait_is_consumed() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    assert_eq!(unsafe { nros_platform_wake_signal(buf.as_mut_ptr()) }, 0);
    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 5_000) };
    let elapsed = t0.elapsed();

    assert_eq!(rc, 0, "expected signaled (rc=0), got {}", rc);
    assert!(
        elapsed < Duration::from_millis(100),
        "wait did not return promptly after pre-signal: {:?}",
        elapsed
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

#[test]
fn cross_thread_signal_wakes_waiter() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    // Smuggle the pointer across the thread boundary as a usize so
    // the borrow checker doesn't insist on Send for a `*mut c_void`.
    let raw_addr = buf.as_mut_ptr() as usize;

    let signaller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        unsafe { nros_platform_wake_signal(raw_addr as *mut c_void) };
    });

    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_wait_ms(raw_addr as *mut c_void, 5_000) };
    let elapsed = t0.elapsed();

    signaller.join().unwrap();

    assert_eq!(rc, 0, "expected signaled (rc=0), got {}", rc);
    assert!(
        elapsed >= Duration::from_millis(80),
        "wait returned before signal could be sent: {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "wait took too long to observe signal: {:?}",
        elapsed
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

#[test]
fn signal_coalesces_when_already_pending() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    // Two signals before any wait — second should be a no-op
    // (semaphore stays at value 1).
    assert_eq!(unsafe { nros_platform_wake_signal(buf.as_mut_ptr()) }, 0);
    assert_eq!(unsafe { nros_platform_wake_signal(buf.as_mut_ptr()) }, 0);

    // First wait consumes the signal.
    assert_eq!(
        unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) },
        0
    );
    // Second wait must time out — the coalesced signal didn't bump
    // the sem past 1.
    assert_eq!(
        unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) },
        1
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

#[test]
fn signal_from_isr_aliases_to_signal_on_hosted_posix() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    assert_eq!(
        unsafe { nros_platform_wake_signal_from_isr(buf.as_mut_ptr()) },
        0
    );
    assert_eq!(
        unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) },
        0
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

// phase-436 C2 — the microsecond park. What it exists for is the one thing the
// millisecond ABI cannot do: a park SHORTER than a millisecond that still
// blocks. The old path either rounded such a request up to a whole millisecond
// or (truncated) did not wait at all, so the two bounds below are each the
// half one of those defects would fail.

/// The granularity is the resolution of the clock the park's deadline is
/// measured on, rounded up to whole microseconds — not the millisecond the ABI
/// signature suggests (issue 1242). Asserted against the clock itself, so the
/// test states the rule rather than one host's number.
#[test]
fn park_granularity_is_the_monotonic_clock_resolution() {
    let mut res = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(
        unsafe { libc::clock_getres(libc::CLOCK_MONOTONIC, &mut res) },
        0
    );
    let ns = res.tv_sec as u64 * 1_000_000_000 + res.tv_nsec as u64;
    let expected = ns.div_ceil(1000).max(1);
    let g = unsafe { nros_platform_wake_park_granularity_us() };
    assert_eq!(
        g, expected,
        "park granularity must be the clock's, rounded up"
    );
    assert!(
        g >= 1,
        "a granularity of zero would claim infinite resolution"
    );
}

/// A 300 us park that nobody signals: never returns before its deadline
/// (lower bound — the truncating path returned at once), and CAN return well
/// inside a millisecond (upper bound — the millisecond ABI could not).
///
/// The upper bound is on the FASTEST of several parks, because one park under
/// a loaded parallel test run can be scheduled late for reasons that say
/// nothing about the primitive; the lower bound holds on every one.
#[test]
fn park_sub_millisecond_deadline_is_honoured_both_ways() {
    let mut buf = WakeBuf::new();
    assert_eq!(unsafe { nros_platform_wake_init(buf.as_mut_ptr()) }, 0);

    const PARK_US: u64 = 300;
    let mut fastest = Duration::MAX;
    for _ in 0..20 {
        let t0 = Instant::now();
        let rc = unsafe { nros_platform_wake_park_until_us(buf.as_mut_ptr(), PARK_US) };
        let elapsed = t0.elapsed();
        assert_eq!(
            rc, 1,
            "an unsignalled park must report its deadline expired"
        );
        assert!(
            elapsed >= Duration::from_micros(PARK_US),
            "park returned before its deadline: {elapsed:?} < {PARK_US} us"
        );
        fastest = fastest.min(elapsed);
    }
    assert!(
        fastest < Duration::from_micros(1_000),
        "no 300 us park finished inside a millisecond (fastest {fastest:?}) — \
         the park is still floored at the millisecond ABI"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// The park waits on the SAME semaphore `signal` posts, so a signal from
/// another thread breaks a long park early and reports 0.
#[test]
fn park_is_broken_by_a_cross_thread_signal() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let raw_addr = buf.as_mut_ptr() as usize;

    let signaller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        unsafe { nros_platform_wake_signal(raw_addr as *mut c_void) };
    });

    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_park_until_us(raw_addr as *mut c_void, 5_000_000) };
    let elapsed = t0.elapsed();
    signaller.join().unwrap();

    assert_eq!(rc, 0, "a signalled park must report 0, got {rc}");
    assert!(
        elapsed >= Duration::from_millis(40),
        "park returned before the signal could be sent: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "park did not observe the signal: {elapsed:?}"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// A zero deadline is a non-blocking take: 1 with nothing pending, 0 (and
/// consumed) with a signal pending.
#[test]
fn park_zero_deadline_is_a_non_blocking_take() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    assert_eq!(
        unsafe { nros_platform_wake_park_until_us(buf.as_mut_ptr(), 0) },
        1
    );
    assert_eq!(unsafe { nros_platform_wake_signal(buf.as_mut_ptr()) }, 0);
    assert_eq!(
        unsafe { nros_platform_wake_park_until_us(buf.as_mut_ptr(), 0) },
        0
    );
    assert_eq!(
        unsafe { nros_platform_wake_park_until_us(buf.as_mut_ptr(), 0) },
        1,
        "the signal must have been consumed by the first take"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}
