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
    assert_eq!(unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) }, 0);
    // Second wait must time out — the coalesced signal didn't bump
    // the sem past 1.
    assert_eq!(unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) }, 1);

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}

#[test]
fn signal_from_isr_aliases_to_signal_on_hosted_posix() {
    let mut buf = WakeBuf::new();
    unsafe { nros_platform_wake_init(buf.as_mut_ptr()) };

    assert_eq!(unsafe { nros_platform_wake_signal_from_isr(buf.as_mut_ptr()) }, 0);
    assert_eq!(unsafe { nros_platform_wake_wait_ms(buf.as_mut_ptr(), 50) }, 0);

    unsafe { nros_platform_wake_drop(buf.as_mut_ptr()) };
}
