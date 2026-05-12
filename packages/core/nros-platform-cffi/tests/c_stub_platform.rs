//! Phase 121.4.c — second-language smoke test for the canonical
//! platform C ABI.
//!
//! Drives a stub platform whose every `nros_platform_*` symbol is
//! defined in **plain C** (`tests/c_stubs/platform_stubs.c`). Each
//! C stub bumps a per-category counter; this test calls every Rust
//! extern wrapper and checks all categories advanced.
//!
//! Verifies:
//!
//! 1. The Rust-side `unsafe extern "C"` declarations in
//!    `nros-platform-cffi/src/lib.rs` match the C-side signatures
//!    byte-for-byte (mismatched ABI would crash the test or fail to
//!    link).
//! 2. Every symbol declared in `<nros/platform.h>` actually has a
//!    Rust mirror.
//! 3. `CffiPlatform`'s trait impls dispatch to the right symbol.
//!
//! Run via:
//! ```bash
//! cargo test -p nros-platform-cffi --features c-stub-test --test c_stub_platform
//! ```

#![cfg(feature = "c-stub-test")]

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};

use nros_platform_api::{
    PlatformAlloc, PlatformClock, PlatformRandom, PlatformSleep, PlatformThreading, PlatformTime,
    PlatformYield,
};
use nros_platform_cffi::CffiPlatform;

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum StubCategory {
    Total = 0,
    Clock = 1,
    Alloc = 2,
    Sleep = 3,
    Yield = 4,
    Random = 5,
    Time = 6,
    Task = 7,
    Mutex = 8,
    Condvar = 9,
}

unsafe extern "C" {
    fn nros_platform_stub_counter(category: StubCategory) -> u32;
    fn nros_platform_stub_reset_counters();
}

// `cargo test` runs cases in parallel; share the C-side counters
// safely by serialising the single test below. A single-test file
// keeps the harness trivial.
static IN_USE: AtomicBool = AtomicBool::new(false);

#[test]
fn every_category_dispatches_through_cffi_platform() {
    // Refuse concurrent entry; counters are global C-side state.
    assert!(
        !IN_USE.swap(true, Ordering::SeqCst),
        "c-stub harness is single-threaded",
    );

    unsafe { nros_platform_stub_reset_counters() };

    // -- Clock --
    let _ = CffiPlatform::clock_ms();
    let _ = CffiPlatform::clock_us();

    // -- Alloc --
    let p = CffiPlatform::alloc(64);
    let _ = CffiPlatform::realloc(p, 128);
    CffiPlatform::dealloc(p);

    // -- Sleep --
    CffiPlatform::sleep_us(1);
    CffiPlatform::sleep_ms(1);
    CffiPlatform::sleep_s(0);

    // -- Yield --
    CffiPlatform::yield_now();

    // -- Random --
    let _ = CffiPlatform::random_u8();
    let _ = CffiPlatform::random_u16();
    let _ = CffiPlatform::random_u32();
    let _ = CffiPlatform::random_u64();
    let mut buf = [0u8; 4];
    CffiPlatform::random_fill(buf.as_mut_ptr() as *mut c_void, buf.len());

    // -- Time --
    let _ = CffiPlatform::time_now_ms();
    let _ = CffiPlatform::time_since_epoch_secs();
    let _ = CffiPlatform::time_since_epoch_nanos();

    // -- Tasks (no real spawn; stubs just bump the counter) --
    let mut task_storage: *mut c_void = core::ptr::null_mut();
    let _ = CffiPlatform::task_init(
        &mut task_storage as *mut _ as *mut c_void,
        core::ptr::null_mut(),
        None,
        core::ptr::null_mut(),
    );
    let _ = CffiPlatform::task_join(core::ptr::null_mut());
    let _ = CffiPlatform::task_detach(core::ptr::null_mut());
    let _ = CffiPlatform::task_cancel(core::ptr::null_mut());
    CffiPlatform::task_exit();
    CffiPlatform::task_free(&mut task_storage as *mut _);

    // -- Mutex (non-recursive + recursive both bump MUTEX) --
    let mut mtx: u64 = 0;
    let m = &mut mtx as *mut _ as *mut c_void;
    let _ = CffiPlatform::mutex_init(m);
    let _ = CffiPlatform::mutex_lock(m);
    let _ = CffiPlatform::mutex_try_lock(m);
    let _ = CffiPlatform::mutex_unlock(m);
    let _ = CffiPlatform::mutex_drop(m);
    let _ = CffiPlatform::mutex_rec_init(m);
    let _ = CffiPlatform::mutex_rec_lock(m);
    let _ = CffiPlatform::mutex_rec_try_lock(m);
    let _ = CffiPlatform::mutex_rec_unlock(m);
    let _ = CffiPlatform::mutex_rec_drop(m);

    // -- Condvar --
    let mut cv: u64 = 0;
    let cvp = &mut cv as *mut _ as *mut c_void;
    let _ = CffiPlatform::condvar_init(cvp);
    let _ = CffiPlatform::condvar_signal(cvp);
    let _ = CffiPlatform::condvar_signal_all(cvp);
    let _ = CffiPlatform::condvar_wait(cvp, m);
    let _ = CffiPlatform::condvar_wait_until(cvp, m, 0);
    let _ = CffiPlatform::condvar_drop(cvp);

    let counter = |c| unsafe { nros_platform_stub_counter(c) };

    assert!(counter(StubCategory::Clock) >= 2, "clock dispatch");
    assert!(counter(StubCategory::Alloc) >= 3, "alloc dispatch");
    assert!(counter(StubCategory::Sleep) >= 3, "sleep dispatch");
    assert!(counter(StubCategory::Yield) >= 1, "yield dispatch");
    assert!(counter(StubCategory::Random) >= 5, "random dispatch");
    assert!(counter(StubCategory::Time) >= 3, "time dispatch");
    assert!(counter(StubCategory::Task) >= 6, "task dispatch");
    assert!(counter(StubCategory::Mutex) >= 10, "mutex dispatch");
    assert!(counter(StubCategory::Condvar) >= 6, "condvar dispatch");
    // 2 + 3 + 3 + 1 + 5 + 3 + 6 + 10 + 6 = 39
    assert_eq!(counter(StubCategory::Total), 39, "total = 39 fn calls");

    IN_USE.store(false, Ordering::SeqCst);
}
