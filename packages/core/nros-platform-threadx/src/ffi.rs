#![allow(dead_code)]
//! ThreadX C API declarations.
//!
//! These are resolved at link time from the ThreadX kernel library
//! linked by the board crate.

use core::ffi::c_void;

/// ThreadX success return code.
pub const TX_SUCCESS: u32 = 0;
/// Infinite wait.
pub const TX_WAIT_FOREVER: u32 = 0xFFFFFFFF;
/// Non-blocking.
pub const TX_NO_WAIT: u32 = 0;

unsafe extern "C" {
    // Clock
    pub fn tx_time_get() -> u32;

    // Memory (byte pool)
    // The board crate must provide the byte pool pointer via set_byte_pool().
    pub fn tx_byte_allocate(
        pool: *mut c_void,
        ptr: *mut *mut c_void,
        size: u32,
        wait_option: u32,
    ) -> u32;
    pub fn tx_byte_release(ptr: *mut c_void) -> u32;

    // Sleep
    pub fn tx_thread_sleep(ticks: u32) -> u32;

    // Threads
    pub fn tx_thread_create(
        thread: *mut c_void,
        name: *const core::ffi::c_char,
        entry: unsafe extern "C" fn(u32),
        entry_input: u32,
        stack: *mut c_void,
        stack_size: u32,
        priority: u32,
        preempt_threshold: u32,
        time_slice: u32,
        auto_start: u32,
    ) -> u32;

    // Mutex
    pub fn tx_mutex_create(mutex: *mut c_void, name: *const core::ffi::c_char, inherit: u32)
    -> u32;
    pub fn tx_mutex_delete(mutex: *mut c_void) -> u32;
    pub fn tx_mutex_get(mutex: *mut c_void, wait_option: u32) -> u32;
    pub fn tx_mutex_put(mutex: *mut c_void) -> u32;

    // Semaphore (for condvar emulation)
    pub fn tx_semaphore_create(
        sem: *mut c_void,
        name: *const core::ffi::c_char,
        initial_count: u32,
    ) -> u32;
    pub fn tx_semaphore_delete(sem: *mut c_void) -> u32;
    pub fn tx_semaphore_get(sem: *mut c_void, wait_option: u32) -> u32;
    pub fn tx_semaphore_put(sem: *mut c_void) -> u32;
}
