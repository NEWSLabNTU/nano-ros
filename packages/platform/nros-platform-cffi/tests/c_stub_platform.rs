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

use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
};

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
    // phase-352 retired `clock_ms` / `clock_us` for one nanosecond symbol plus a
    // declared resolution. This test still called the removed pair, which no
    // build noticed because nothing enables `c-stub-test` by default — and
    // `check-retired-platform-clock-symbols` could not notice either: it scans
    // C/C++ sources, so a retired symbol in Rust is outside its reach.
    let _ = CffiPlatform::clock_ns();
    let _ = CffiPlatform::clock_resolution_ns();
    // issue 0758 — the wall-clock epoch is a CLOCK-category op too, and this
    // test's claim is that EVERY category dispatches through CffiPlatform. A
    // new ABI fn that no case calls makes that claim quietly narrower than it
    // reads.
    let _ = CffiPlatform::epoch_us();

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
    let _ = CffiPlatform::time_now_ns();

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

    assert!(counter(StubCategory::Clock) >= 3, "clock dispatch");
    assert!(counter(StubCategory::Alloc) >= 3, "alloc dispatch");
    assert!(counter(StubCategory::Sleep) >= 3, "sleep dispatch");
    assert!(counter(StubCategory::Yield) >= 1, "yield dispatch");
    assert!(counter(StubCategory::Random) >= 5, "random dispatch");
    // issue 0779 — was `>= 3`. `PlatformTime` has exactly ONE method
    // (`time_now_ns`) and this test calls it once; the trait was reduced and the
    // expectation was not. Nothing caught it because the whole file sits behind
    // `#![cfg(feature = "c-stub-test")]`, which no recipe enabled — so this
    // assertion had not run since the reduction. Derived from the call list
    // above, not guessed.
    assert!(counter(StubCategory::Time) >= 1, "time dispatch");
    assert!(counter(StubCategory::Task) >= 6, "task dispatch");
    assert!(counter(StubCategory::Mutex) >= 10, "mutex dispatch");
    assert!(counter(StubCategory::Condvar) >= 6, "condvar dispatch");
    // clock 3 (ns, resolution, epoch) + alloc 3 + sleep 3 + yield 1 + random 5
    // (4x random_u + random_fill) + time 1 + task 6 + mutex 10 + condvar 6 = 38.
    // Each number is the count of DISTINCT calls in the body above; recount
    // there before changing one here.
    assert_eq!(counter(StubCategory::Total), 38, "total = 38 fn calls");

    IN_USE.store(false, Ordering::SeqCst);
}
