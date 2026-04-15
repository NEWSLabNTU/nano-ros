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

mod ffi;
mod types;

/// Zero-sized type implementing all platform methods for FreeRTOS.
pub struct FreeRtosPlatform;

// ============================================================================
// Clock — xTaskGetTickCount
// ============================================================================

impl FreeRtosPlatform {
    #[inline]
    pub fn clock_ms() -> u64 {
        let ticks = unsafe { ffi::xTaskGetTickCount() };
        // portTICK_PERIOD_MS is typically 1 for configTICK_RATE_HZ=1000.
        // We assume 1ms ticks; board crates can override if different.
        ticks as u64
    }

    #[inline]
    pub fn clock_us() -> u64 {
        Self::clock_ms() * 1000
    }
}

// ============================================================================
// Memory — pvPortMalloc / vPortFree
// ============================================================================

impl FreeRtosPlatform {
    #[inline]
    pub fn alloc(size: usize) -> *mut c_void {
        unsafe { ffi::pvPortMalloc(size) }
    }

    #[inline]
    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        // FreeRTOS has no realloc — allocate new, copy, free old.
        if ptr.is_null() {
            return Self::alloc(size);
        }
        let new_ptr = Self::alloc(size);
        if !new_ptr.is_null() {
            // We don't know the old size, so copy `size` bytes (caller's
            // responsibility to ensure old allocation >= size).
            unsafe { core::ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, size) };
            Self::dealloc(ptr);
        }
        new_ptr
    }

    #[inline]
    pub fn dealloc(ptr: *mut c_void) {
        unsafe { ffi::vPortFree(ptr) }
    }
}

// ============================================================================
// Sleep — vTaskDelay
// ============================================================================

impl FreeRtosPlatform {
    #[inline]
    pub fn sleep_us(us: usize) {
        Self::sleep_ms(us.div_ceil(1000));
    }

    #[inline]
    pub fn sleep_ms(ms: usize) {
        unsafe { ffi::vTaskDelay(ms as u32) };
    }

    #[inline]
    pub fn sleep_s(s: usize) {
        Self::sleep_ms(s * 1000);
    }
}

// ============================================================================
// Random — xorshift (same as bare-metal)
// ============================================================================

static mut RNG_STATE: u32 = 0x12345678;

/// Seed the PRNG.
pub fn seed(value: u32) {
    unsafe { RNG_STATE = if value == 0 { 0x12345678 } else { value } }
}

fn next_u32() -> u32 {
    unsafe {
        let mut x = RNG_STATE;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        RNG_STATE = x;
        x
    }
}

impl FreeRtosPlatform {
    pub fn random_u8() -> u8 {
        (next_u32() & 0xFF) as u8
    }
    pub fn random_u16() -> u16 {
        (next_u32() & 0xFFFF) as u16
    }
    pub fn random_u32() -> u32 {
        next_u32()
    }
    pub fn random_u64() -> u64 {
        ((next_u32() as u64) << 32) | next_u32() as u64
    }

    pub fn random_fill(buf: *mut c_void, len: usize) {
        if buf.is_null() {
            return;
        }
        let ptr = buf as *mut u8;
        let mut offset = 0;
        let mut remaining = len;
        while remaining >= 4 {
            let bytes = next_u32().to_ne_bytes();
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), 4) };
            offset += 4;
            remaining -= 4;
        }
        if remaining > 0 {
            let bytes = next_u32().to_ne_bytes();
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.add(offset), remaining) };
        }
    }
}

// ============================================================================
// Time — monotonic (no RTC)
// ============================================================================

impl FreeRtosPlatform {
    #[inline]
    pub fn time_now_ms() -> u64 {
        Self::clock_ms()
    }
    #[inline]
    pub fn time_since_epoch_secs() -> u32 {
        (Self::clock_ms() / 1000) as u32
    }
    #[inline]
    pub fn time_since_epoch_nanos() -> u32 {
        ((Self::clock_ms() % 1000) * 1_000_000) as u32
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
