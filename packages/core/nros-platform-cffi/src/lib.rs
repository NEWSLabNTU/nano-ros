//! C vtable adapter for the nros platform abstraction.
//!
//! This crate provides a vtable-based bridge so that platform implementations
//! written in C (or any language with a C ABI) can satisfy the nros platform
//! traits without writing Rust code.
//!
//! # Usage (C platform implementor)
//!
//! 1. Include `<nros/platform_vtable.h>`
//! 2. Fill in all function pointers in `nros_platform_vtable_t`
//! 3. Call `nros_platform_cffi_register(&my_vtable)` before opening a session
//!
//! # Usage (Rust consumer)
//!
//! Enable the `platform-cffi` feature on `nros-platform`. The
//! [`CffiPlatform`] zero-sized type implements all platform traits by
//! dispatching through the registered vtable.

#![no_std]

use core::ffi::c_void;
use core::sync::atomic::Ordering;

use portable_atomic::AtomicPtr;

// ============================================================================
// Vtable definition (mirrors C header)
// ============================================================================

/// C function table for a platform implementation.
///
/// All function pointers are required. For capabilities the platform does
/// not support (e.g., threading on bare-metal), provide stubs that return 0
/// (success) for mutex/condvar ops and -1 for `task_init`.
#[repr(C)]
pub struct NrosPlatformVtable {
    // -- Clock --
    pub clock_ms: unsafe extern "C" fn() -> u64,
    pub clock_us: unsafe extern "C" fn() -> u64,

    // -- Alloc --
    pub alloc: unsafe extern "C" fn(size: usize) -> *mut c_void,
    pub realloc: unsafe extern "C" fn(ptr: *mut c_void, size: usize) -> *mut c_void,
    pub dealloc: unsafe extern "C" fn(ptr: *mut c_void),

    // -- Sleep --
    pub sleep_us: unsafe extern "C" fn(us: usize),
    pub sleep_ms: unsafe extern "C" fn(ms: usize),
    pub sleep_s: unsafe extern "C" fn(s: usize),

    // -- Random --
    pub random_u8: unsafe extern "C" fn() -> u8,
    pub random_u16: unsafe extern "C" fn() -> u16,
    pub random_u32: unsafe extern "C" fn() -> u32,
    pub random_u64: unsafe extern "C" fn() -> u64,
    pub random_fill: unsafe extern "C" fn(buf: *mut c_void, len: usize),

    // -- Time (wall clock) --
    pub time_now_ms: unsafe extern "C" fn() -> u64,
    pub time_since_epoch_secs: unsafe extern "C" fn() -> u32,
    pub time_since_epoch_nanos: unsafe extern "C" fn() -> u32,

    // -- Threading --
    pub task_init: unsafe extern "C" fn(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8,
    pub task_join: unsafe extern "C" fn(task: *mut c_void) -> i8,
    pub task_detach: unsafe extern "C" fn(task: *mut c_void) -> i8,
    pub task_cancel: unsafe extern "C" fn(task: *mut c_void) -> i8,
    pub task_exit: unsafe extern "C" fn(),
    pub task_free: unsafe extern "C" fn(task: *mut *mut c_void),

    pub mutex_init: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_drop: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_try_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_unlock: unsafe extern "C" fn(m: *mut c_void) -> i8,

    pub mutex_rec_init: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_rec_drop: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_rec_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_rec_try_lock: unsafe extern "C" fn(m: *mut c_void) -> i8,
    pub mutex_rec_unlock: unsafe extern "C" fn(m: *mut c_void) -> i8,

    pub condvar_init: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    pub condvar_drop: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    pub condvar_signal: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    pub condvar_signal_all: unsafe extern "C" fn(cv: *mut c_void) -> i8,
    pub condvar_wait: unsafe extern "C" fn(cv: *mut c_void, m: *mut c_void) -> i8,
    pub condvar_wait_until: unsafe extern "C" fn(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8,
}

// ============================================================================
// Registration
// ============================================================================

static VTABLE: AtomicPtr<NrosPlatformVtable> = AtomicPtr::new(core::ptr::null_mut());

/// Register a platform vtable.
///
/// # Safety
///
/// The vtable pointer must remain valid for the lifetime of the program.
/// All function pointers in the vtable must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_platform_cffi_register(vtable: *const NrosPlatformVtable) -> i32 {
    VTABLE.store(vtable as *mut NrosPlatformVtable, Ordering::Release);
    0
}

fn get_vtable() -> &'static NrosPlatformVtable {
    let ptr = VTABLE.load(Ordering::Acquire);
    assert!(!ptr.is_null(), "nros_platform_cffi_register() not called");
    // SAFETY: Registration ensures the pointer is valid and 'static.
    unsafe { &*ptr }
}

// ============================================================================
// CffiPlatform — implements all traits by dispatching through the vtable
// ============================================================================

/// Zero-sized type that implements platform traits via a registered C vtable.
pub struct CffiPlatform;

// We can't directly depend on nros-platform (circular), so the trait impls
// are written in nros-platform's resolve module via a wrapper, or the shim
// crates call these methods directly.

impl CffiPlatform {
    // -- Clock --
    #[inline]
    pub fn clock_ms() -> u64 {
        unsafe { (get_vtable().clock_ms)() }
    }

    #[inline]
    pub fn clock_us() -> u64 {
        unsafe { (get_vtable().clock_us)() }
    }

    // -- Alloc --
    #[inline]
    pub fn alloc(size: usize) -> *mut c_void {
        unsafe { (get_vtable().alloc)(size) }
    }

    #[inline]
    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        unsafe { (get_vtable().realloc)(ptr, size) }
    }

    #[inline]
    pub fn dealloc(ptr: *mut c_void) {
        unsafe { (get_vtable().dealloc)(ptr) }
    }

    // -- Sleep --
    #[inline]
    pub fn sleep_us(us: usize) {
        unsafe { (get_vtable().sleep_us)(us) }
    }

    #[inline]
    pub fn sleep_ms(ms: usize) {
        unsafe { (get_vtable().sleep_ms)(ms) }
    }

    #[inline]
    pub fn sleep_s(s: usize) {
        unsafe { (get_vtable().sleep_s)(s) }
    }

    // -- Random --
    #[inline]
    pub fn random_u8() -> u8 {
        unsafe { (get_vtable().random_u8)() }
    }

    #[inline]
    pub fn random_u16() -> u16 {
        unsafe { (get_vtable().random_u16)() }
    }

    #[inline]
    pub fn random_u32() -> u32 {
        unsafe { (get_vtable().random_u32)() }
    }

    #[inline]
    pub fn random_u64() -> u64 {
        unsafe { (get_vtable().random_u64)() }
    }

    #[inline]
    pub fn random_fill(buf: *mut c_void, len: usize) {
        unsafe { (get_vtable().random_fill)(buf, len) }
    }

    // -- Time --
    #[inline]
    pub fn time_now_ms() -> u64 {
        unsafe { (get_vtable().time_now_ms)() }
    }

    #[inline]
    pub fn time_since_epoch_secs() -> u32 {
        unsafe { (get_vtable().time_since_epoch_secs)() }
    }

    #[inline]
    pub fn time_since_epoch_nanos() -> u32 {
        unsafe { (get_vtable().time_since_epoch_nanos)() }
    }

    // -- Threading --
    #[inline]
    pub fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        unsafe { (get_vtable().task_init)(task, attr, entry, arg) }
    }

    #[inline]
    pub fn task_join(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_join)(task) }
    }

    #[inline]
    pub fn task_detach(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_detach)(task) }
    }

    #[inline]
    pub fn task_cancel(task: *mut c_void) -> i8 {
        unsafe { (get_vtable().task_cancel)(task) }
    }

    #[inline]
    pub fn task_exit() {
        unsafe { (get_vtable().task_exit)() }
    }

    #[inline]
    pub fn task_free(task: *mut *mut c_void) {
        unsafe { (get_vtable().task_free)(task) }
    }

    #[inline]
    pub fn mutex_init(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_init)(m) }
    }

    #[inline]
    pub fn mutex_drop(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_drop)(m) }
    }

    #[inline]
    pub fn mutex_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_lock)(m) }
    }

    #[inline]
    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_try_lock)(m) }
    }

    #[inline]
    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_unlock)(m) }
    }

    #[inline]
    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_init)(m) }
    }

    #[inline]
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_drop)(m) }
    }

    #[inline]
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_lock)(m) }
    }

    #[inline]
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_try_lock)(m) }
    }

    #[inline]
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        unsafe { (get_vtable().mutex_rec_unlock)(m) }
    }

    #[inline]
    pub fn condvar_init(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_init)(cv) }
    }

    #[inline]
    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_drop)(cv) }
    }

    #[inline]
    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_signal)(cv) }
    }

    #[inline]
    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_signal_all)(cv) }
    }

    #[inline]
    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        unsafe { (get_vtable().condvar_wait)(cv, m) }
    }

    #[inline]
    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        unsafe { (get_vtable().condvar_wait_until)(cv, m, abstime) }
    }
}
