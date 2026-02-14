//! Threading stubs for zenoh-pico (single-threaded bare-metal)
//!
//! All threading primitives are no-ops since Z_FEATURE_MULTI_THREAD=0.
//! Task init returns an error to prevent accidental thread creation.

use core::ffi::c_void;

// ============================================================================
// Types (opaque — zenoh-pico only passes pointers, never inspects contents)
// ============================================================================

/// Opaque task type (unused on single-threaded platforms)
#[repr(C)]
pub struct ZTask {
    _unused: u8,
}

/// Opaque task attribute type
#[repr(C)]
pub struct ZTaskAttr {
    _unused: u8,
}

/// Opaque mutex type
#[repr(C)]
pub struct ZMutex {
    _unused: u8,
}

/// Opaque recursive mutex type
#[repr(C)]
pub struct ZMutexRec {
    _unused: u8,
}

/// Opaque condition variable type
#[repr(C)]
pub struct ZCondvar {
    _unused: u8,
}

// ============================================================================
// Task stubs
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_init(
    _task: *mut ZTask,
    _attr: *mut ZTaskAttr,
    _fun: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    _arg: *mut c_void,
) -> i8 {
    -1 // _Z_ERR_GENERIC — cannot create threads on single-threaded platform
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_join(_task: *mut ZTask) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_detach(_task: *mut ZTask) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_cancel(_task: *mut ZTask) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_exit() {
    // No-op
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_task_free(_task: *mut *mut ZTask) {
    // No-op
}

// ============================================================================
// Mutex stubs
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_init(_m: *mut ZMutex) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_drop(_m: *mut ZMutex) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_lock(_m: *mut ZMutex) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_try_lock(_m: *mut ZMutex) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_unlock(_m: *mut ZMutex) -> i8 {
    0
}

// ============================================================================
// Recursive mutex stubs
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_init(_m: *mut ZMutexRec) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_drop(_m: *mut ZMutexRec) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_lock(_m: *mut ZMutexRec) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_try_lock(_m: *mut ZMutexRec) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_mutex_rec_unlock(_m: *mut ZMutexRec) -> i8 {
    0
}

// ============================================================================
// Condition variable stubs
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_init(_cv: *mut ZCondvar) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_drop(_cv: *mut ZCondvar) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_signal(_cv: *mut ZCondvar) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_signal_all(_cv: *mut ZCondvar) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_wait(_cv: *mut ZCondvar, _m: *mut ZMutex) -> i8 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn _z_condvar_wait_until(
    _cv: *mut ZCondvar,
    _m: *mut ZMutex,
    _abstime: *const u64,
) -> i8 {
    0
}
