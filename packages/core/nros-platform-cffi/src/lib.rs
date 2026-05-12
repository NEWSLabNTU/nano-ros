//! Rust mirror of the canonical C ABI in `<nros/platform.h>`.
//!
//! Every nros binary links exactly one platform implementation; the
//! free `extern "C"` symbols declared below are resolved at link time.
//! There is no runtime registration step. To inject a platform from
//! C, drop a translation unit defining the symbols (or link against
//! a static library that does).
//!
//! Rust platform crates implement the [`nros_platform_api`] traits as
//! before; a sibling `-cffi` shim crate re-exports the Rust impl as
//! `#[unsafe(no_mangle)] extern "C"` symbols matching the names in the
//! header. That separation lets the same Rust impl serve both
//! trait-driven Rust callers and C-ABI consumers.
//!
//! # Usage
//!
//! - C implementor: implement the functions in `<nros/platform.h>` and
//!   link against the nros binary.
//! - Rust consumer: enable the `platform-cffi` feature on
//!   `nros-platform`; [`CffiPlatform`] dispatches every trait call to
//!   the linked C symbols.
//!
//! # Companion
//!
//! Platform sits one tier below RMW. The Phase 117 RMW vtable
//! (`<nros/rmw_vtable.h>`) is a runtime-pluggable struct; the
//! platform layer is link-time-bound free symbols. Different choice
//! because RMW backends genuinely swap per session (zenoh vs cyclonedds
//! vs xrce in the same binary at test time) while a platform is fixed
//! for the life of a binary.

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;

// ============================================================================
// Canonical ABI declarations
// ----------------------------------------------------------------------------
// Hand-written mirror of `include/nros/platform.h`. Field order, names,
// and types track the header byte-for-byte. Updates land in the header
// first, then here.
// ============================================================================

unsafe extern "C" {
    // -- Clock --
    pub fn nros_platform_clock_ms() -> u64;
    pub fn nros_platform_clock_us() -> u64;

    // -- Alloc --
    pub fn nros_platform_alloc(size: usize) -> *mut c_void;
    pub fn nros_platform_realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    pub fn nros_platform_dealloc(ptr: *mut c_void);

    // -- Sleep --
    pub fn nros_platform_sleep_us(us: usize);
    pub fn nros_platform_sleep_ms(ms: usize);
    pub fn nros_platform_sleep_s(s: usize);

    // -- Yield --
    pub fn nros_platform_yield_now();

    // -- Random --
    pub fn nros_platform_random_u8() -> u8;
    pub fn nros_platform_random_u16() -> u16;
    pub fn nros_platform_random_u32() -> u32;
    pub fn nros_platform_random_u64() -> u64;
    pub fn nros_platform_random_fill(buf: *mut c_void, len: usize);

    // -- Time (wall clock) --
    pub fn nros_platform_time_now_ms() -> u64;
    pub fn nros_platform_time_since_epoch_secs() -> u32;
    pub fn nros_platform_time_since_epoch_nanos() -> u32;

    // -- Tasks --
    pub fn nros_platform_task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8;
    pub fn nros_platform_task_join(task: *mut c_void) -> i8;
    pub fn nros_platform_task_detach(task: *mut c_void) -> i8;
    pub fn nros_platform_task_cancel(task: *mut c_void) -> i8;
    pub fn nros_platform_task_exit();
    pub fn nros_platform_task_free(task: *mut *mut c_void);

    // -- Mutex (non-recursive) --
    pub fn nros_platform_mutex_init(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_drop(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_lock(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_try_lock(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_unlock(m: *mut c_void) -> i8;

    // -- Mutex (recursive) --
    pub fn nros_platform_mutex_rec_init(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_rec_drop(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_rec_lock(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_rec_try_lock(m: *mut c_void) -> i8;
    pub fn nros_platform_mutex_rec_unlock(m: *mut c_void) -> i8;

    // -- Condvar --
    pub fn nros_platform_condvar_init(cv: *mut c_void) -> i8;
    pub fn nros_platform_condvar_drop(cv: *mut c_void) -> i8;
    pub fn nros_platform_condvar_signal(cv: *mut c_void) -> i8;
    pub fn nros_platform_condvar_signal_all(cv: *mut c_void) -> i8;
    pub fn nros_platform_condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8;
    pub fn nros_platform_condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8;
}

// ============================================================================
// Return codes (mirrors header)
// ============================================================================

/// Mirrors C `nros_platform_ret_t`.
pub type NrosPlatformRet = i32;

pub const NROS_PLATFORM_RET_OK: NrosPlatformRet = 0;
pub const NROS_PLATFORM_RET_ERROR: NrosPlatformRet = -1;
pub const NROS_PLATFORM_RET_UNSUPPORTED: NrosPlatformRet = -5;

// ============================================================================
// CffiPlatform — trait impls dispatching to the linked C symbols
// ============================================================================

/// Zero-sized type implementing the platform traits via the canonical
/// `nros_platform_*` C symbols.
///
/// The crate that pulls `CffiPlatform` into a final binary is
/// responsible for ensuring the symbols are supplied at link time
/// (either by a C translation unit or a Rust `-cffi` shim crate).
pub struct CffiPlatform;

impl nros_platform_api::PlatformClock for CffiPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        unsafe { nros_platform_clock_ms() }
    }

    #[inline]
    fn clock_us() -> u64 {
        unsafe { nros_platform_clock_us() }
    }
}

impl nros_platform_api::PlatformAlloc for CffiPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        unsafe { nros_platform_alloc(size) }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        unsafe { nros_platform_realloc(ptr, size) }
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        unsafe { nros_platform_dealloc(ptr) }
    }
}

impl nros_platform_api::PlatformSleep for CffiPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        unsafe { nros_platform_sleep_us(us) }
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        unsafe { nros_platform_sleep_ms(ms) }
    }

    #[inline]
    fn sleep_s(s: usize) {
        unsafe { nros_platform_sleep_s(s) }
    }
}

impl nros_platform_api::PlatformYield for CffiPlatform {
    #[inline]
    fn yield_now() {
        unsafe { nros_platform_yield_now() }
    }
}

// Phase 110.D — `PlatformScheduler` is satisfied by the existing yield
// symbol; per-thread scheduling controls land when a C consumer needs
// hard-RT preemption.
impl nros_platform_api::PlatformScheduler for CffiPlatform {
    #[inline]
    fn yield_now() {
        unsafe { nros_platform_yield_now() }
    }
}

impl nros_platform_api::PlatformRandom for CffiPlatform {
    #[inline]
    fn random_u8() -> u8 {
        unsafe { nros_platform_random_u8() }
    }

    #[inline]
    fn random_u16() -> u16 {
        unsafe { nros_platform_random_u16() }
    }

    #[inline]
    fn random_u32() -> u32 {
        unsafe { nros_platform_random_u32() }
    }

    #[inline]
    fn random_u64() -> u64 {
        unsafe { nros_platform_random_u64() }
    }

    #[inline]
    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe { nros_platform_random_fill(buf, len) }
    }
}

impl nros_platform_api::PlatformTime for CffiPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        unsafe { nros_platform_time_now_ms() }
    }

    #[inline]
    fn time_since_epoch_secs() -> u32 {
        unsafe { nros_platform_time_since_epoch_secs() }
    }

    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        unsafe { nros_platform_time_since_epoch_nanos() }
    }
}

impl nros_platform_api::PlatformThreading for CffiPlatform {
    fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        unsafe { nros_platform_task_init(task, attr, entry, arg) }
    }
    fn task_join(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_join(task) }
    }
    fn task_detach(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_detach(task) }
    }
    fn task_cancel(task: *mut c_void) -> i8 {
        unsafe { nros_platform_task_cancel(task) }
    }
    fn task_exit() {
        unsafe { nros_platform_task_exit() }
    }
    fn task_free(task: *mut *mut c_void) {
        unsafe { nros_platform_task_free(task) }
    }
    fn mutex_init(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_init(m) }
    }
    fn mutex_drop(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_drop(m) }
    }
    fn mutex_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_lock(m) }
    }
    fn mutex_try_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_try_lock(m) }
    }
    fn mutex_unlock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_unlock(m) }
    }
    fn mutex_rec_init(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_init(m) }
    }
    fn mutex_rec_drop(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_drop(m) }
    }
    fn mutex_rec_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_lock(m) }
    }
    fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_try_lock(m) }
    }
    fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        unsafe { nros_platform_mutex_rec_unlock(m) }
    }
    fn condvar_init(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_init(cv) }
    }
    fn condvar_drop(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_drop(cv) }
    }
    fn condvar_signal(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_signal(cv) }
    }
    fn condvar_signal_all(cv: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_signal_all(cv) }
    }
    fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        unsafe { nros_platform_condvar_wait(cv, m) }
    }
    fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        unsafe { nros_platform_condvar_wait_until(cv, m, abstime) }
    }
}

// ============================================================================
// Test-only stubs
// ----------------------------------------------------------------------------
// `cargo test -p nros-platform-cffi` builds a test binary that links
// the rlib. Without these, the unresolved extern symbols above would
// fail to link even when no test exercises `CffiPlatform`. Real
// platform crates supply their own definitions and never compile this
// module (it is gated on `cfg(test)`).
// ============================================================================

#[cfg(test)]
mod test_stubs {
    use core::ffi::c_void;

    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_clock_ms() -> u64 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_clock_us() -> u64 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_alloc(_: usize) -> *mut c_void {
        core::ptr::null_mut()
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_realloc(_: *mut c_void, _: usize) -> *mut c_void {
        core::ptr::null_mut()
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_dealloc(_: *mut c_void) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_sleep_us(_: usize) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_sleep_ms(_: usize) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_sleep_s(_: usize) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_yield_now() {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_random_u8() -> u8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_random_u16() -> u16 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_random_u32() -> u32 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_random_u64() -> u64 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_random_fill(_: *mut c_void, _: usize) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_time_now_ms() -> u64 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_time_since_epoch_secs() -> u32 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_time_since_epoch_nanos() -> u32 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_init(
        _: *mut c_void,
        _: *mut c_void,
        _: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        _: *mut c_void,
    ) -> i8 {
        -1
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_join(_: *mut c_void) -> i8 {
        -1
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_detach(_: *mut c_void) -> i8 {
        -1
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_cancel(_: *mut c_void) -> i8 {
        -1
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_exit() {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_task_free(_: *mut *mut c_void) {}
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_init(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_drop(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_lock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_try_lock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_unlock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_rec_init(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_rec_drop(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_rec_lock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_rec_try_lock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_mutex_rec_unlock(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_init(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_drop(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_signal(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_signal_all(_: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_wait(_: *mut c_void, _: *mut c_void) -> i8 {
        0
    }
    #[unsafe(no_mangle)]
    extern "C" fn nros_platform_condvar_wait_until(_: *mut c_void, _: *mut c_void, _: u64) -> i8 {
        0
    }
}
