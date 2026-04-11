#![allow(dead_code)]
//! ThreadX C API declarations.
//!
//! These are resolved at link time from the ThreadX kernel library
//! linked by the board crate.
//!
//! ThreadX exposes its API as macros (`tx_mutex_create` → `_tx_mutex_create`).
//! Rust FFI can't call C macros, so we declare the underlying `_tx_*` functions
//! directly (same approach as nros-platform-freertos with FreeRTOS macros).

use core::ffi::c_void;

/// ThreadX success return code.
pub const TX_SUCCESS: u32 = 0;
/// Infinite wait.
pub const TX_WAIT_FOREVER: u32 = 0xFFFFFFFF;
/// Non-blocking.
pub const TX_NO_WAIT: u32 = 0;

unsafe extern "C" {
    // Clock
    #[link_name = "_tx_time_get"]
    pub fn tx_time_get() -> u32;

    // Memory (byte pool)
    #[link_name = "_tx_byte_allocate"]
    pub fn tx_byte_allocate(
        pool: *mut c_void,
        ptr: *mut *mut c_void,
        size: u32,
        wait_option: u32,
    ) -> u32;
    #[link_name = "_tx_byte_release"]
    pub fn tx_byte_release(ptr: *mut c_void) -> u32;

    // Sleep
    #[link_name = "_tx_thread_sleep"]
    pub fn tx_thread_sleep(ticks: u32) -> u32;

    // Threads
    #[link_name = "_tx_thread_create"]
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

    #[link_name = "_tx_thread_info_get"]
    pub fn tx_thread_info_get(
        thread: *mut c_void,
        name: *mut *const core::ffi::c_char,
        state: *mut u32,
        run_count: *mut u32,
        priority: *mut u32,
        preempt_threshold: *mut u32,
        time_slice: *mut u32,
        next_thread: *mut *mut c_void,
        next_suspended: *mut *mut c_void,
    ) -> u32;

    // Mutex
    #[link_name = "_tx_mutex_create"]
    pub fn tx_mutex_create(mutex: *mut c_void, name: *const core::ffi::c_char, inherit: u32)
    -> u32;
    #[link_name = "_tx_mutex_delete"]
    pub fn tx_mutex_delete(mutex: *mut c_void) -> u32;
    #[link_name = "_tx_mutex_get"]
    pub fn tx_mutex_get(mutex: *mut c_void, wait_option: u32) -> u32;
    #[link_name = "_tx_mutex_put"]
    pub fn tx_mutex_put(mutex: *mut c_void) -> u32;

    // Semaphore (for condvar emulation)
    #[link_name = "_tx_semaphore_create"]
    pub fn tx_semaphore_create(
        sem: *mut c_void,
        name: *const core::ffi::c_char,
        initial_count: u32,
    ) -> u32;
    #[link_name = "_tx_semaphore_delete"]
    pub fn tx_semaphore_delete(sem: *mut c_void) -> u32;
    #[link_name = "_tx_semaphore_get"]
    pub fn tx_semaphore_get(sem: *mut c_void, wait_option: u32) -> u32;
    #[link_name = "_tx_semaphore_put"]
    pub fn tx_semaphore_put(sem: *mut c_void) -> u32;
}
