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
    io::Write,
    mem::MaybeUninit,
    os::{fd::AsRawFd, unix::net::UnixStream},
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
    fn nros_platform_wake_park_handles_us(
        w: *mut c_void,
        handles: *const i32,
        n: usize,
        deadline_us: u64,
    ) -> i8;
    fn nros_platform_wake_storage_size() -> usize;
    fn nros_platform_wake_storage_align() -> usize;
}

// Caller allocates aligned storage sized at runtime. Use a generous
// fixed-size MaybeUninit buffer that covers `sem_t` on every POSIX
// target we care about (sem_t is ~32 bytes on Linux x86_64, ~16 on
// macOS pthread_cond_t+mutex+flag layout). 256 bytes is overkill on
// purpose so the test doesn't have to allocate. The runtime probe
// (`nros_platform_wake_storage_size()`) is asserted below.
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
///
/// Under heavy load even the fastest of 20 parks can miss the millisecond
/// (issue 2195: fastest 3.9 ms at load average 38; the same test passes solo),
/// so an absolute bound alone measures the host. Each park is therefore paired
/// with a plain 300 us `thread::sleep` taken right before it, which suffers the
/// same wake-up latency, and the bound also passes when the MEDIAN of the
/// per-pair differences (park minus sleep) is under `FLOOR_MARGIN_US`.
///
/// What that buys, measured with the park mutated to round up to whole
/// milliseconds: on a quiet host the mutation fails every run (median ~780 us);
/// with 96 busy loops (load average 35) the unmutated park passed 5 of 5 and
/// the mutation still failed 3 of 5 (median 700 us). So under load the bound
/// can miss the regression, never invent one; a quiet run still catches it.
#[test]
fn park_sub_millisecond_deadline_is_honoured_both_ways() {
    let mut buf = WakeBuf::new();
    assert_eq!(unsafe { nros_platform_wake_init(buf.as_mut_ptr()) }, 0);

    const PARK_US: u64 = 300;
    const PAIRS: usize = 41;
    const FLOOR_MARGIN_US: i128 = 350;
    let mut fastest = Duration::MAX;
    let mut diffs_us: Vec<i128> = Vec::with_capacity(PAIRS);
    for _ in 0..PAIRS {
        let t0 = Instant::now();
        thread::sleep(Duration::from_micros(PARK_US));
        let slept = t0.elapsed();

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
        diffs_us.push(elapsed.as_micros() as i128 - slept.as_micros() as i128);
    }
    diffs_us.sort_unstable();
    let median_us = diffs_us[PAIRS / 2];
    assert!(
        fastest < Duration::from_micros(1_000) || median_us < FLOOR_MARGIN_US,
        "no 300 us park finished inside a millisecond (fastest {fastest:?}), and a \
         park ran a median {median_us} us longer than a 300 us sleep beside it \
         (bound {FLOOR_MARGIN_US} us) — the park is still floored at the \
         millisecond ABI"
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

// ---------------------------------------------------------------------------
// phase-521 W3 (RFC-0113 D3) — the park over the wake object AND OS handles.
// ---------------------------------------------------------------------------

/// A handle becoming readable ends the park, from another thread, with 0 —
/// and the park does not read it (the session drains its own transport).
#[test]
fn handle_park_is_ended_by_a_readable_handle() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let (rx, mut tx) = UnixStream::pair().unwrap();
    let fd = rx.as_raw_fd();

    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        tx.write_all(&[7]).unwrap();
        tx
    });
    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_park_handles_us(buf.as_mut_ptr(), &fd, 1, 5_000_000) };
    let elapsed = t0.elapsed();
    let _tx = writer.join().unwrap();

    assert_eq!(
        rc, 0,
        "a readable handle must end the park with 0, got {rc}"
    );
    assert!(
        elapsed >= Duration::from_millis(40),
        "returned before the write: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "missed the write: {elapsed:?}"
    );
    let mut byte = [0u8; 1];
    assert_eq!(
        unsafe { libc::recv(fd, byte.as_mut_ptr().cast(), 1, libc::MSG_DONTWAIT) },
        1,
        "the park consumed the handle's data; the session must find it there"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// The wake object still ends a handle park: a guard condition or a Notify
/// session signals it, and the park sees the signal through its self-pipe.
#[test]
fn handle_park_is_ended_by_a_cross_thread_signal() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let (rx, _tx) = UnixStream::pair().unwrap();
    let fd = rx.as_raw_fd();
    let raw_addr = buf.as_mut_ptr() as usize;

    let signaller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        unsafe { nros_platform_wake_signal(raw_addr as *mut c_void) };
    });
    let t0 = Instant::now();
    let rc =
        unsafe { nros_platform_wake_park_handles_us(raw_addr as *mut c_void, &fd, 1, 5_000_000) };
    let elapsed = t0.elapsed();
    signaller.join().unwrap();

    assert_eq!(rc, 0, "a signalled park must report 0, got {rc}");
    assert!(elapsed >= Duration::from_millis(40), "{elapsed:?}");
    assert!(
        elapsed < Duration::from_millis(1_000),
        "missed the signal: {elapsed:?}"
    );
    assert_eq!(
        unsafe { nros_platform_wake_park_until_us(raw_addr as *mut c_void, 0) },
        1,
        "the handle park must consume the signal it woke on"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// With nothing readable and nothing signalled, the park waits out its deadline
/// and reports 1 — no earlier.
#[test]
fn handle_park_times_out_at_its_deadline() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let (rx, _tx) = UnixStream::pair().unwrap();
    let fd = rx.as_raw_fd();

    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_park_handles_us(buf.as_mut_ptr(), &fd, 1, 30_000) };
    let elapsed = t0.elapsed();

    assert_eq!(rc, 1, "an idle park must time out with 1, got {rc}");
    assert!(
        elapsed >= Duration::from_millis(30),
        "woke early: {elapsed:?}"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// A signal taken by a plain `wait_ms` leaves its pipe byte behind. That byte
/// is stale and must not end the next handle park early.
#[test]
fn a_stale_pipe_byte_does_not_end_the_handle_park() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let (rx, _tx) = UnixStream::pair().unwrap();
    let fd = rx.as_raw_fd();

    // Make the self-pipe (the first handle park does), then signal and take
    // the signal through the semaphore wait, which leaves the byte.
    assert_eq!(
        unsafe { nros_platform_wake_park_handles_us(buf.as_mut_ptr(), &fd, 1, 0) },
        1
    );
    assert_eq!(unsafe { nros_platform_wake_signal(buf.as_mut_ptr()) }, 0);
    assert_eq!(
        unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 0) },
        0
    );

    let t0 = Instant::now();
    let rc = unsafe { nros_platform_wake_park_handles_us(buf.as_mut_ptr(), &fd, 1, 30_000) };
    let elapsed = t0.elapsed();
    assert_eq!(rc, 1, "a stale byte ended the park: {rc}");
    assert!(
        elapsed >= Duration::from_millis(30),
        "woke early on a stale byte: {elapsed:?}"
    );

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

/// More handles than the port multiplexes is refused, never truncated: a
/// dropped handle would be a missed wake.
#[test]
fn handle_park_refuses_more_handles_than_it_can_watch() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };
    let fds = [0i32; 9];
    assert_eq!(
        unsafe { nros_platform_wake_park_handles_us(buf.as_mut_ptr(), fds.as_ptr(), fds.len(), 0) },
        -1
    );
    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}
