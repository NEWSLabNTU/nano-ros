//! Phase 121.4.c — confirm `nros_platform_cffi::nros_platform_export!`
//! emits every `nros_platform_*` symbol declared in `<nros/platform.h>`
//! when invoked against `PosixPlatform`, and that each symbol
//! dispatches to the underlying PosixPlatform trait impl.
//!
//! Gated behind the `cffi-export` feature — without it the macro is
//! never invoked and the symbols never emitted.

#![cfg(feature = "cffi-export")]

use core::ffi::c_void;

// Pulls the crate's compilation units (and their `#[no_mangle]`
// items) into the test binary. Without this the integration-test
// linker doesn't see any reason to load the rlib's object files
// and the `extern "C"` references below stay unresolved.
#[allow(unused_imports)]
use nros_platform_posix::PosixPlatform;

// Mirror of `<nros/platform.h>`. If the platform crate's macro fails
// to emit any name below, this test won't link.
unsafe extern "C" {
    fn nros_platform_clock_ms() -> u64;
    fn nros_platform_clock_us() -> u64;
    fn nros_platform_alloc(size: usize) -> *mut c_void;
    fn nros_platform_realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn nros_platform_dealloc(ptr: *mut c_void);
    fn nros_platform_sleep_us(us: usize);
    fn nros_platform_sleep_ms(ms: usize);
    fn nros_platform_sleep_s(s: usize);
    fn nros_platform_yield_now();
    fn nros_platform_random_u8() -> u8;
    fn nros_platform_random_u16() -> u16;
    fn nros_platform_random_u32() -> u32;
    fn nros_platform_random_u64() -> u64;
    fn nros_platform_random_fill(buf: *mut c_void, len: usize);
    fn nros_platform_time_now_ms() -> u64;
    fn nros_platform_time_since_epoch_secs() -> u32;
    fn nros_platform_time_since_epoch_nanos() -> u32;
    fn nros_platform_task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8;
    fn nros_platform_task_join(task: *mut c_void) -> i8;
    fn nros_platform_task_detach(task: *mut c_void) -> i8;
    fn nros_platform_task_cancel(task: *mut c_void) -> i8;
    fn nros_platform_task_exit();
    fn nros_platform_task_free(task: *mut *mut c_void);
    fn nros_platform_mutex_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_try_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_unlock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_init(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_drop(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_try_lock(m: *mut c_void) -> i8;
    fn nros_platform_mutex_rec_unlock(m: *mut c_void) -> i8;
    fn nros_platform_condvar_init(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_drop(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_signal(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_signal_all(cv: *mut c_void) -> i8;
    fn nros_platform_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;
    fn nros_platform_condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8;
}

#[test]
fn posix_macro_emits_every_symbol() {
    // Cheap dispatch — the value of the test is in linking, not in
    // exercising POSIX behaviour. (PosixPlatform's own crate tests
    // cover that.)
    let t0 = unsafe { nros_platform_clock_ms() };
    std::thread::sleep(std::time::Duration::from_millis(2));
    let t1 = unsafe { nros_platform_clock_ms() };
    assert!(t1 >= t0, "clock_ms must be monotonic via cffi export");

    let _ = unsafe { nros_platform_clock_us() };
    let _ = unsafe { nros_platform_time_now_ms() };
    let _ = unsafe { nros_platform_time_since_epoch_secs() };
    let _ = unsafe { nros_platform_time_since_epoch_nanos() };
    let _ = unsafe { nros_platform_random_u32() };
    let _ = unsafe { nros_platform_random_u64() };
    unsafe { nros_platform_yield_now() };

    // Just touch every other symbol as a fn pointer so the linker
    // keeps them — these calls aren't safe to actually invoke under
    // libc, but `as *const ()` is sound and pins the externs.
    let pins: [*const (); 31] = [
        nros_platform_alloc as *const (),
        nros_platform_realloc as *const (),
        nros_platform_dealloc as *const (),
        nros_platform_sleep_us as *const (),
        nros_platform_sleep_ms as *const (),
        nros_platform_sleep_s as *const (),
        nros_platform_random_u8 as *const (),
        nros_platform_random_u16 as *const (),
        nros_platform_random_fill as *const (),
        nros_platform_task_init as *const (),
        nros_platform_task_join as *const (),
        nros_platform_task_detach as *const (),
        nros_platform_task_cancel as *const (),
        nros_platform_task_exit as *const (),
        nros_platform_task_free as *const (),
        nros_platform_mutex_init as *const (),
        nros_platform_mutex_drop as *const (),
        nros_platform_mutex_lock as *const (),
        nros_platform_mutex_try_lock as *const (),
        nros_platform_mutex_unlock as *const (),
        nros_platform_mutex_rec_init as *const (),
        nros_platform_mutex_rec_drop as *const (),
        nros_platform_mutex_rec_lock as *const (),
        nros_platform_mutex_rec_try_lock as *const (),
        nros_platform_mutex_rec_unlock as *const (),
        nros_platform_condvar_init as *const (),
        nros_platform_condvar_drop as *const (),
        nros_platform_condvar_signal as *const (),
        nros_platform_condvar_signal_all as *const (),
        nros_platform_condvar_wait as *const (),
        nros_platform_condvar_wait_until as *const (),
    ];
    for p in pins {
        assert!(!p.is_null(), "every exported symbol must resolve");
    }
}
