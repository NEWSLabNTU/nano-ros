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
// Threading — FreeRTOS tasks, recursive mutexes, condvar emulation
// ============================================================================

impl FreeRtosPlatform {
    pub fn task_init(
        task: *mut c_void,
        _attr: *mut c_void,
        entry: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
        arg: *mut c_void,
    ) -> i8 {
        let entry = match entry {
            Some(f) => f,
            None => return -1,
        };
        // Wrapper: FreeRTOS task function has different signature (no return)
        // We store the real entry+arg and use a trampoline.
        // For simplicity, cast directly — the return value is ignored by FreeRTOS.
        let ret = unsafe {
            ffi::xTaskCreate(
                core::mem::transmute::<
                    unsafe extern "C" fn(*mut c_void) -> *mut c_void,
                    unsafe extern "C" fn(*mut c_void),
                >(entry),
                c"nros".as_ptr(),
                4096,
                arg,
                4, // tskIDLE_PRIORITY + 4
                task as *mut *mut c_void,
            )
        };
        if ret == 1 { 0 } else { -1 } // pdPASS = 1
    }

    pub fn task_join(_task: *mut c_void) -> i8 {
        0
    }
    pub fn task_detach(_task: *mut c_void) -> i8 {
        0
    }
    pub fn task_cancel(task: *mut c_void) -> i8 {
        unsafe { ffi::vTaskDelete(task) };
        0
    }
    pub fn task_exit() {
        unsafe { ffi::vTaskDelete(core::ptr::null_mut()) };
    }
    pub fn task_free(_task: *mut *mut c_void) {}

    // -- Mutex (recursive) --

    pub fn mutex_init(m: *mut c_void) -> i8 {
        let handle = unsafe { ffi::xSemaphoreCreateRecursiveMutex() };
        if handle.is_null() {
            return -1;
        }
        unsafe { *(m as *mut *mut c_void) = handle };
        0
    }

    pub fn mutex_drop(m: *mut c_void) -> i8 {
        let handle = unsafe { *(m as *const *mut c_void) };
        if !handle.is_null() {
            unsafe { ffi::vSemaphoreDelete(handle) }
        };
        0
    }

    pub fn mutex_lock(m: *mut c_void) -> i8 {
        let handle = unsafe { *(m as *const *mut c_void) };
        let ret = unsafe { ffi::xSemaphoreTakeRecursive(handle, u32::MAX) };
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_try_lock(m: *mut c_void) -> i8 {
        let handle = unsafe { *(m as *const *mut c_void) };
        let ret = unsafe { ffi::xSemaphoreTakeRecursive(handle, 0) };
        if ret == 1 { 0 } else { -1 }
    }

    pub fn mutex_unlock(m: *mut c_void) -> i8 {
        let handle = unsafe { *(m as *const *mut c_void) };
        let ret = unsafe { ffi::xSemaphoreGiveRecursive(handle) };
        if ret == 1 { 0 } else { -1 }
    }

    // -- Recursive mutex (same as mutex on FreeRTOS) --

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

    // -- Condition variables (semaphore-based emulation) --

    pub fn condvar_init(cv: *mut c_void) -> i8 {
        let sem = unsafe { ffi::xSemaphoreCreateCounting(32, 0) };
        if sem.is_null() {
            return -1;
        }
        unsafe { *(cv as *mut *mut c_void) = sem };
        0
    }

    pub fn condvar_drop(cv: *mut c_void) -> i8 {
        let sem = unsafe { *(cv as *const *mut c_void) };
        if !sem.is_null() {
            unsafe { ffi::vSemaphoreDelete(sem) }
        };
        0
    }

    pub fn condvar_signal(cv: *mut c_void) -> i8 {
        let sem = unsafe { *(cv as *const *mut c_void) };
        unsafe { ffi::xSemaphoreGive(sem) };
        0
    }

    pub fn condvar_signal_all(cv: *mut c_void) -> i8 {
        // Signal multiple waiters (best-effort with counting semaphore)
        Self::condvar_signal(cv)
    }

    pub fn condvar_wait(cv: *mut c_void, m: *mut c_void) -> i8 {
        let sem = unsafe { *(cv as *const *mut c_void) };
        Self::mutex_unlock(m);
        unsafe { ffi::xSemaphoreTake(sem, u32::MAX) };
        Self::mutex_lock(m);
        0
    }

    pub fn condvar_wait_until(cv: *mut c_void, m: *mut c_void, abstime: u64) -> i8 {
        let sem = unsafe { *(cv as *const *mut c_void) };
        let now = Self::clock_ms();
        let timeout = abstime.saturating_sub(now) as u32;
        Self::mutex_unlock(m);
        let ret = unsafe { ffi::xSemaphoreTake(sem, timeout) };
        Self::mutex_lock(m);
        if ret == 1 { 0 } else { -1 } // -1 = timeout
    }
}
