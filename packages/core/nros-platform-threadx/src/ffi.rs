#![allow(dead_code)]
//! ThreadX C API declarations.
//!
//! These are resolved at link time from the ThreadX kernel library
//! linked by the board crate.
//!
//! ThreadX exposes its API as macros (`tx_mutex_create` → `_tx_mutex_create`).
//! Rust FFI can't call C macros, so we declare the underlying `_tx_*` functions
//! directly (same approach as nros-platform-freertos with FreeRTOS macros).
//!
//! ABI note: ThreadX `ULONG` is **4 bytes on every port supported here**.
//! - Cortex-M: native `unsigned long` is 4 bytes (ILP32).
//! - Linux x86_64 (upstream `tx_port.h`): `#if defined(__x86_64__) typedef
//!   unsigned int ULONG`. The upstream port deliberately makes ULONG 4
//!   bytes on LP64 to keep TCB / pool / message-queue offset arithmetic
//!   stable; the comment in our board override
//!   (`packages/boards/nros-board-threadx-qemu-riscv64/config/tx_port.h`)
//!   notes the kernel code uses `ULONG *` pointer arithmetic assuming
//!   4-byte words.
//! - ThreadX RV64 (board override): same `typedef unsigned int ULONG`
//!   for the same reason.
//!
//! So `TxUlong = u32` across every threadx target. The Phase 120.3
//! commit `57669baf` mistakenly aliased `TxUlong` to `c_ulong` (u64 on
//! LP64). That happened to work for small values because the RV64 ABI
//! zero-extends u32 → u64 in the argument register, but it caused
//! systematic 4-byte over-reservation on every `tx_byte_allocate` and
//! widened the wait-option / count fields out of band — the corrected
//! width is `u32`, matching the C side.
//!
//! Pointer arguments stay `*mut c_void` (8 bytes on LP64, 4 on ILP32);
//! ULONG-typed scalar arguments (size, ticks, wait_option, count) are
//! `u32`.

use core::ffi::c_void;

/// Matches ThreadX `ULONG` width across every supported port (Cortex-M,
/// Linux x86_64, RV64). Always 4 bytes; do not change to `c_ulong`.
pub type TxUlong = u32;

/// ThreadX success return code.
pub const TX_SUCCESS: u32 = 0;
/// Infinite wait — `(ULONG)0xFFFFFFFFUL` in `tx_api.h`. The literal is the
/// 32-bit pattern; ThreadX promotes to ULONG, which is what callers must
/// pass. On LP64 this is `0x00000000_FFFFFFFF`, NOT `0xFFFFFFFF_FFFFFFFF`.
pub const TX_WAIT_FOREVER: TxUlong = 0xFFFFFFFF;
/// Non-blocking.
pub const TX_NO_WAIT: TxUlong = 0;

unsafe extern "C" {
    // Clock
    #[link_name = "_tx_time_get"]
    pub fn tx_time_get() -> TxUlong;

    // Memory (byte pool)
    #[link_name = "_tx_byte_allocate"]
    pub fn tx_byte_allocate(
        pool: *mut c_void,
        ptr: *mut *mut c_void,
        size: TxUlong,
        wait_option: TxUlong,
    ) -> u32;
    #[link_name = "_tx_byte_release"]
    pub fn tx_byte_release(ptr: *mut c_void) -> u32;

    // Sleep
    #[link_name = "_tx_thread_sleep"]
    pub fn tx_thread_sleep(ticks: TxUlong) -> u32;

    // Cooperative yield (Phase 77.22)
    #[link_name = "_tx_thread_relinquish"]
    pub fn tx_thread_relinquish();

    // Phase 110.D — per-thread scheduling controls.
    #[link_name = "_tx_thread_identify"]
    pub fn tx_thread_identify() -> *mut c_void;
    #[link_name = "_tx_thread_priority_change"]
    pub fn tx_thread_priority_change(
        thread: *mut c_void,
        new_priority: u32,
        old_priority: *mut u32,
    ) -> u32;
    #[link_name = "_tx_thread_preemption_change"]
    pub fn tx_thread_preemption_change(
        thread: *mut c_void,
        new_threshold: u32,
        old_threshold: *mut u32,
    ) -> u32;
    /// Phase 110.D — only present in ThreadX SMP builds. Gate the
    /// extern decl behind `feature = "threadx-smp"` so single-core
    /// targets (QEMU, ThreadX-Linux) don't reference an unresolved
    /// symbol at link time.
    #[cfg(feature = "threadx-smp")]
    #[link_name = "_tx_thread_smp_core_exclude"]
    pub fn tx_thread_smp_core_exclude(thread: *mut c_void, exclude_map: u32) -> u32;

    // Phase 110.E.b — application timers. `tx_timer_create`'s
    // `expiration_input` is a `ULONG` (32-bit on every supported
    // ThreadX target — even the 64-bit ports keep ULONG = u32 for
    // ABI compat). We pack a leaked Bridge pointer through this
    // slot — works only on 32-bit targets; 64-bit ports need a
    // static cookie→bridge slab (deferred).
    #[link_name = "_tx_timer_create"]
    pub fn tx_timer_create(
        timer: *mut c_void,
        name: *const core::ffi::c_char,
        expiration: extern "C" fn(u32),
        expiration_input: u32,
        initial_ticks: u32,
        reschedule_ticks: u32,
        auto_activate: u32,
    ) -> u32;
    #[link_name = "_tx_timer_delete"]
    pub fn tx_timer_delete(timer: *mut c_void) -> u32;
    #[link_name = "_tx_timer_deactivate"]
    pub fn tx_timer_deactivate(timer: *mut c_void) -> u32;

    // Threads
    #[link_name = "_tx_thread_create"]
    pub fn tx_thread_create(
        thread: *mut c_void,
        name: *const core::ffi::c_char,
        entry: unsafe extern "C" fn(TxUlong),
        entry_input: TxUlong,
        stack: *mut c_void,
        stack_size: TxUlong,
        priority: u32,
        preempt_threshold: u32,
        time_slice: TxUlong,
        auto_start: u32,
    ) -> u32;

    #[link_name = "_tx_thread_info_get"]
    pub fn tx_thread_info_get(
        thread: *mut c_void,
        name: *mut *const core::ffi::c_char,
        state: *mut u32,
        run_count: *mut TxUlong,
        priority: *mut u32,
        preempt_threshold: *mut u32,
        time_slice: *mut TxUlong,
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
    pub fn tx_mutex_get(mutex: *mut c_void, wait_option: TxUlong) -> u32;
    #[link_name = "_tx_mutex_put"]
    pub fn tx_mutex_put(mutex: *mut c_void) -> u32;

    // Semaphore (for condvar emulation)
    #[link_name = "_tx_semaphore_create"]
    pub fn tx_semaphore_create(
        sem: *mut c_void,
        name: *const core::ffi::c_char,
        initial_count: TxUlong,
    ) -> u32;
    #[link_name = "_tx_semaphore_delete"]
    pub fn tx_semaphore_delete(sem: *mut c_void) -> u32;
    #[link_name = "_tx_semaphore_get"]
    pub fn tx_semaphore_get(sem: *mut c_void, wait_option: TxUlong) -> u32;
    #[link_name = "_tx_semaphore_put"]
    pub fn tx_semaphore_put(sem: *mut c_void) -> u32;
}
