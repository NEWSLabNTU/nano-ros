//! nros platform implementation for FreeRTOS.
//!
//! Calls FreeRTOS C APIs (`xTaskCreate`, `pvPortMalloc`, `xSemaphoreCreateRecursiveMutex`,
//! etc.) via `extern "C"` declarations resolved at link time from the FreeRTOS
//! kernel library linked by the board crate.
//!
//! # Threading model
//!
//! FreeRTOS uses:
//! - Tasks: `xTaskCreate` / `vTaskDelete`
//! - Mutexes: recursive semaphores (`xSemaphoreCreateRecursiveMutex`)
//! - Condition variables: manual implementation using counting semaphore + mutex
//! - Task join: event groups (`xEventGroupWaitBits`)

#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_void;
use nros_platform_api::{PlatformAlloc, PlatformClock};

mod ffi;
pub mod net;
mod types;

/// Zero-sized type implementing all platform methods for FreeRTOS.
pub struct FreeRtosPlatform;

// ============================================================================
// Clock — xTaskGetTickCount
// ============================================================================

impl nros_platform_api::PlatformClock for FreeRtosPlatform {
    #[inline]
    fn clock_ms() -> u64 {
        let ticks = unsafe { ffi::xTaskGetTickCount() };
        // portTICK_PERIOD_MS is typically 1 for configTICK_RATE_HZ=1000.
        // We assume 1ms ticks; board crates can override if different.
        ticks as u64
    }

    #[inline]
    fn clock_us() -> u64 {
        <Self as PlatformClock>::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — pvPortMalloc / vPortFree
// ============================================================================

impl nros_platform_api::PlatformAlloc for FreeRtosPlatform {
    #[inline]
    fn alloc(size: usize) -> *mut c_void {
        unsafe { ffi::pvPortMalloc(size) }
    }

    #[inline]
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        use nros_platform_api::PlatformAlloc;
        // FreeRTOS has no realloc — allocate new, copy, free old.
        if ptr.is_null() {
            return <Self as PlatformAlloc>::alloc(size);
        }
        let new_ptr = <Self as PlatformAlloc>::alloc(size);
        if !new_ptr.is_null() {
            // We don't know the old size, so copy `size` bytes (caller's
            // responsibility to ensure old allocation >= size).
            unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size) };
            <Self as PlatformAlloc>::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    fn dealloc(ptr: *mut c_void) {
        unsafe { ffi::vPortFree(ptr) }
    }
}

// ============================================================================
// Sleep — vTaskDelay
// ============================================================================

impl nros_platform_api::PlatformSleep for FreeRtosPlatform {
    #[inline]
    fn sleep_us(us: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    fn sleep_ms(ms: usize) {
        unsafe { ffi::vTaskDelay(ms as u32) };
    }

    #[inline]
    fn sleep_s(s: usize) {
        use nros_platform_api::PlatformSleep;
        <Self as PlatformSleep>::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Yield — 1-tick vTaskDelay (approximation)
// ============================================================================

impl nros_platform_api::PlatformYield for FreeRtosPlatform {
    #[inline]
    fn yield_now() {
        // FreeRTOS's true cooperative yield is the C macro `taskYIELD()`
        // (which expands to `portYIELD()` — a port-specific inline asm
        // that triggers PendSV on Cortex-M). Calling a macro from Rust
        // FFI would need a one-line C shim compiled against FreeRTOS
        // headers (deferred — see Phase 77.22 note).
        //
        // `vTaskDelay(1)` blocks for one scheduler tick, which is
        // effectively a yield: the scheduler picks the highest-priority
        // ready task; if only the caller is runnable it resumes after
        // the tick. `vTaskDelay(0)` is a documented no-op in FreeRTOS,
        // so we pass 1.
        unsafe { ffi::vTaskDelay(1) };
    }
}

// ============================================================================
// Random — xorshift (shared helpers from nros_platform_api::xorshift32)
// ============================================================================

static mut RNG_STATE: u32 = nros_platform_api::xorshift32::DEFAULT_SEED;

/// Seed the PRNG.
pub fn seed(value: u32) {
    unsafe { nros_platform_api::xorshift32::seed(&raw mut RNG_STATE, value) }
}

/// C-callable version for FreeRTOS startup.c to seed the platform RNG.
///
/// Without this, the C examples have no way to inject per-instance
/// entropy into the xorshift state, so two QEMU instances both start
/// with the default `0x12345678` seed → produce identical zenoh
/// session IDs → zenohd treats them as the same peer (`max_links=1`)
/// and rejects the second connection. The Rust path seeds via
/// `seed()` from `app_task_entry`; the C path now mirrors that.
#[unsafe(no_mangle)]
pub extern "C" fn nros_platform_freertos_seed_rng(value: u32) {
    seed(value);
}

fn next_u32() -> u32 {
    unsafe { nros_platform_api::xorshift32::next(&raw mut RNG_STATE) }
}

impl nros_platform_api::PlatformRandom for FreeRtosPlatform {
    fn random_u8() -> u8 {
        (next_u32() & 0xFF) as u8
    }
    fn random_u16() -> u16 {
        (next_u32() & 0xFFFF) as u16
    }
    fn random_u32() -> u32 {
        next_u32()
    }
    fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    fn random_fill(buf: *mut c_void, len: usize) {
        unsafe {
            nros_platform_api::xorshift32::random_fill(&raw mut RNG_STATE, buf as *mut u8, len)
        }
    }
}

// ============================================================================
// Time — monotonic (no RTC)
// ============================================================================

impl nros_platform_api::PlatformTime for FreeRtosPlatform {
    #[inline]
    fn time_now_ms() -> u64 {
        <Self as PlatformClock>::clock_ms()
    }
    #[inline]
    fn time_since_epoch_secs() -> u32 {
        (<Self as PlatformClock>::clock_ms() / 1000) as u32
    }
    #[inline]
    fn time_since_epoch_nanos() -> u32 {
        ((<Self as PlatformClock>::clock_ms() % 1000) * 1_000_000) as u32
    }
}

// ============================================================================
// Threading — FreeRTOS tasks, recursive mutexes, condvar
//
// Uses #[repr(C)] types from types.rs matching zenoh-pico's FreeRTOS structs.
// ============================================================================

/// Task wrapper trampoline — matches zenoh-pico's z_task_wrapper behavior.
/// Reads fun/arg from the _z_task_t struct, runs the function, signals
/// completion via event group, then suspends (deleted by joiner).
unsafe extern "C" fn task_wrapper(arg: *mut c_void) {
    let task = arg as *mut types::ZTask;
    unsafe {
        // Run the task function
        if let Some(fun) = (*task).fun {
            fun((*task).arg);
        }
        // Signal the joiner that the task has finished
        ffi::event_group_set_bits((*task).join_event, 1);
        // Suspend self — joiner will delete us (avoids race with vTaskDelete)
        ffi::task_suspend_current();
    }
}

/// Default task attributes (matches zenoh-pico's z_default_task_attr)
const DEFAULT_TASK_NAME: &core::ffi::CStr = c"nros";
const DEFAULT_PRIORITY: u32 = 3; // configMAX_PRIORITIES / 2
const DEFAULT_STACK_DEPTH: u32 = 5120;

impl FreeRtosPlatform {
    pub fn task_init(
        task: *mut c_void,
        attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        let entry = match entry {
            Some(f) => f,
            None => return -1,
        };

        let t = task as *mut types::ZTask;
        unsafe {
            // Store fun/arg in the struct (task_wrapper reads them)
            (*t).fun = Some(entry);
            (*t).arg = arg;

            // Create join event group
            (*t).join_event = ffi::event_group_create();
            if (*t).join_event.is_null() {
                return -1;
            }

            // Read attributes (or use defaults)
            let (name, priority, stack_depth) = if !attr.is_null() {
                let a = attr as *const types::ZTaskAttr;
                ((*a).name, (*a).priority, (*a).stack_depth as u32)
            } else {
                (
                    DEFAULT_TASK_NAME.as_ptr().cast::<u8>(),
                    DEFAULT_PRIORITY,
                    DEFAULT_STACK_DEPTH,
                )
            };

            // Create the task — pass the ZTask struct as arg to task_wrapper
            let ret = ffi::xTaskCreate(
                task_wrapper,
                name as *const core::ffi::c_char,
                stack_depth,
                task, // task_wrapper receives the ZTask struct
                priority,
                &mut (*t).handle,
            );
            if ret != 1 {
                // pdPASS = 1
                ffi::event_group_delete((*t).join_event);
                return -1;
            }
        }
        0
    }

    pub fn task_join(task: *mut c_void) -> i8 {
        let t = task as *mut types::ZTask;
        unsafe {
            // Wait for task to signal completion
            ffi::event_group_wait_bits((*t).join_event, 1, u32::MAX);

            // Delete the suspended task
            ffi::enter_critical();
            if !(*t).handle.is_null() {
                ffi::vTaskDelete((*t).handle);
                (*t).handle = core::ptr::null_mut();
            }
            ffi::exit_critical();
        }
        0
    }

    pub fn task_detach(_task: *mut c_void) -> i8 {
        -1 // Not supported on FreeRTOS (same as C system.c)
    }

    pub fn task_cancel(task: *mut c_void) -> i8 {
        let t = task as *mut types::ZTask;
        unsafe {
            ffi::enter_critical();
            if !(*t).handle.is_null() {
                ffi::vTaskDelete((*t).handle);
                (*t).handle = core::ptr::null_mut();
            }
            ffi::exit_critical();
            // Signal joiners
            ffi::event_group_set_bits((*t).join_event, 1);
        }
        0
    }

    pub fn task_exit() {
        unsafe { ffi::vTaskDelete(core::ptr::null_mut()) };
    }

    pub fn task_free(task: *mut *mut c_void) {
        unsafe {
            let t = *task as *mut types::ZTask;
            if !t.is_null() {
                ffi::event_group_delete((*t).join_event);
                Self::dealloc(t as *mut c_void);
                *task = core::ptr::null_mut();
            }
        }
    }

    // -- Mutex (recursive) --
    // _z_mutex_t = { SemaphoreHandle_t handle } — single field at offset 0

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let mx = m as *mut types::ZMutex;
        let handle = ffi::create_recursive_mutex();
        if handle.is_null() {
            return -1;
        }
        unsafe { (*mx).handle = handle };
        0
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let handle = unsafe { (*mx).handle };
        if !handle.is_null() {
            ffi::semaphore_delete(handle);
        }
        0
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::take_recursive(unsafe { (*mx).handle }, u32::MAX);
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::take_recursive(unsafe { (*mx).handle }, 0);
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let mx = m as *const types::ZMutex;
        let ret = ffi::give_recursive(unsafe { (*mx).handle });
        if ret == 1 { 0 } else { -1 }
    }

    // -- Recursive mutex (same implementation on FreeRTOS) --

    pub fn mutex_rec_init(m: *mut c_void) -> i8 {
        Self::mutex_init(m)
    }
    pub fn mutex_rec_drop(m: *mut c_void) -> i8 {
        Self::mutex_drop(m)
    }
    pub fn mutex_rec_lock(m: *mut c_void) -> i8 {
        Self::mutex_lock(m)
    }
    pub fn mutex_rec_try_lock(m: *mut c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    pub fn mutex_rec_unlock(m: *mut c_void) -> i8 {
        Self::mutex_unlock(m)
    }

    // -- Condition variables (matching C system.c semantics) --
    // _z_condvar_t = { SemaphoreHandle_t mutex, SemaphoreHandle_t sem, int waiters }

    pub fn condvar_init(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            let mutex = ffi::create_mutex();
            let sem = ffi::create_counting_semaphore(u32::MAX, 0);
            if mutex.is_null() || sem.is_null() {
                if !mutex.is_null() {
                    ffi::semaphore_delete(mutex);
                }
                if !sem.is_null() {
                    ffi::semaphore_delete(sem);
                }
                return -1;
            }
            (*c).mutex = mutex;
            (*c).sem = sem;
            (*c).waiters = 0;
        }
        0
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let c = cv as *const types::ZCondvar;
        unsafe {
            ffi::semaphore_delete((*c).sem);
            ffi::semaphore_delete((*c).mutex);
        }
        0
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            if (*c).waiters > 0 {
                ffi::semaphore_give((*c).sem);
                (*c).waiters -= 1;
            }
            ffi::semaphore_give((*c).mutex);
        }
        0
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            while (*c).waiters > 0 {
                ffi::semaphore_give((*c).sem);
                (*c).waiters -= 1;
            }
            ffi::semaphore_give((*c).mutex);
        }
        0
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        let c = cv as *mut types::ZCondvar;
        unsafe {
            // Increment waiter count
            ffi::semaphore_take((*c).mutex, u32::MAX);
            (*c).waiters += 1;
            ffi::semaphore_give((*c).mutex);

            // Release the caller's mutex and wait on the semaphore
            Self::mutex_unlock(m);
            ffi::semaphore_take((*c).sem, u32::MAX);
            Self::mutex_lock(m);
        }
        0
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        let c = cv as *mut types::ZCondvar;
        let now = Self::clock_ms();
        let timeout = abstime.saturating_sub(now) as u32;

        unsafe {
            ffi::semaphore_take((*c).mutex, u32::MAX);
            (*c).waiters += 1;
            ffi::semaphore_give((*c).mutex);

            Self::mutex_unlock(m);
            let ret = ffi::semaphore_take((*c).sem, timeout);
            Self::mutex_lock(m);

            if ret != 1 {
                // Timed out — decrement waiter count
                ffi::semaphore_take((*c).mutex, u32::MAX);
                (*c).waiters -= 1;
                ffi::semaphore_give((*c).mutex);
                return -1; // Z_ETIMEDOUT
            }
        }
        0
    }
}

impl nros_platform_api::PlatformThreading for FreeRtosPlatform {
    fn task_init(
        task: *mut core::ffi::c_void,
        attr: *mut core::ffi::c_void,
        entry: Option<unsafe extern "C" fn(*mut core::ffi::c_void) -> *mut core::ffi::c_void>,
        arg: *mut core::ffi::c_void,
    ) -> i8 {
        Self::task_init(task, attr, entry, arg)
    }
    fn task_join(task: *mut core::ffi::c_void) -> i8 {
        Self::task_join(task)
    }
    fn task_detach(task: *mut core::ffi::c_void) -> i8 {
        Self::task_detach(task)
    }
    fn task_cancel(task: *mut core::ffi::c_void) -> i8 {
        Self::task_cancel(task)
    }
    fn task_exit() {
        Self::task_exit()
    }
    fn task_free(task: *mut *mut core::ffi::c_void) {
        Self::task_free(task)
    }
    fn mutex_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_init(m)
    }
    fn mutex_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_drop(m)
    }
    fn mutex_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_lock(m)
    }
    fn mutex_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_try_lock(m)
    }
    fn mutex_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_unlock(m)
    }
    fn mutex_rec_init(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_init(m)
    }
    fn mutex_rec_drop(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_drop(m)
    }
    fn mutex_rec_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_lock(m)
    }
    fn mutex_rec_try_lock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_try_lock(m)
    }
    fn mutex_rec_unlock(m: *mut core::ffi::c_void) -> i8 {
        Self::mutex_rec_unlock(m)
    }
    fn condvar_init(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_init(cv)
    }
    fn condvar_drop(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_drop(cv)
    }
    fn condvar_signal(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal(cv)
    }
    fn condvar_signal_all(cv: *mut core::ffi::c_void) -> i8 {
        Self::condvar_signal_all(cv)
    }
    fn condvar_wait(cv: *mut core::ffi::c_void, m: *mut core::ffi::c_void) -> i8 {
        Self::condvar_wait(cv, m)
    }
    fn condvar_wait_until(
        cv: *mut core::ffi::c_void,
        m: *mut core::ffi::c_void,
        abstime: u64,
    ) -> i8 {
        Self::condvar_wait_until(cv, m, abstime)
    }
}
