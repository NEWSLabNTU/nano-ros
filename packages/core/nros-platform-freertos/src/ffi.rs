//! FreeRTOS C API declarations.
//!
//! FreeRTOS exposes many APIs as C macros (xSemaphoreCreateRecursiveMutex,
//! xSemaphoreTake, etc.) that expand to real functions (xQueueCreateMutex,
//! xQueueSemaphoreTake, etc.). We declare the underlying real functions
//! here since Rust FFI can only call real symbols, not macros.

#![allow(dead_code)]

use core::ffi::c_void;

// Queue type constants (from queue.h)
pub const QUEUE_TYPE_MUTEX: u8 = 1;
pub const QUEUE_TYPE_RECURSIVE_MUTEX: u8 = 4;

unsafe extern "C" {
    // Clock
    pub fn xTaskGetTickCount() -> u32;

    // Memory (real functions, not macros)
    pub fn pvPortMalloc(size: usize) -> *mut c_void;
    pub fn vPortFree(ptr: *mut c_void);

    // Sleep (real function)
    pub fn vTaskDelay(ticks: u32);

    // Tasks (real functions)
    pub fn xTaskCreate(
        task_code: unsafe extern "C" fn(*mut c_void),
        name: *const core::ffi::c_char,
        stack_depth: u32,
        parameters: *mut c_void,
        priority: u32,
        task_handle: *mut *mut c_void,
    ) -> i32;
    pub fn vTaskDelete(task_handle: *mut c_void);

    // Event groups (real functions, not macros)
    pub fn xEventGroupCreate() -> *mut c_void;
    pub fn xEventGroupSetBits(group: *mut c_void, bits: u32) -> u32;
    pub fn xEventGroupWaitBits(
        group: *mut c_void,
        bits_to_wait: u32,
        clear_on_exit: i32,
        wait_all: i32,
        ticks_to_wait: u32,
    ) -> u32;
    pub fn vEventGroupDelete(group: *mut c_void);

    // Critical sections (real functions on Cortex-M)
    pub fn vPortEnterCritical();
    pub fn vPortExitCritical();

    // Task suspend (real function)
    pub fn vTaskSuspend(task: *mut c_void);

    // Queue/Semaphore functions (the REAL functions behind the macros)
    //
    // xSemaphoreCreateRecursiveMutex() → xQueueCreateMutex(RECURSIVE_MUTEX)
    // xSemaphoreCreateMutex()          → xQueueCreateMutex(MUTEX)
    // xSemaphoreCreateCounting(m,i)    → xQueueCreateCountingSemaphore(m,i)
    pub fn xQueueCreateMutex(queue_type: u8) -> *mut c_void;
    pub fn xQueueCreateCountingSemaphore(max_count: u32, initial_count: u32) -> *mut c_void;

    // xSemaphoreTakeRecursive(h,t) → xQueueTakeMutexRecursive(h,t)
    pub fn xQueueTakeMutexRecursive(mutex: *mut c_void, ticks_to_wait: u32) -> i32;
    // xSemaphoreGiveRecursive(h) → xQueueGiveMutexRecursive(h)
    pub fn xQueueGiveMutexRecursive(mutex: *mut c_void) -> i32;

    // xSemaphoreTake(h,t) → xQueueSemaphoreTake(h,t)
    pub fn xQueueSemaphoreTake(queue: *mut c_void, ticks_to_wait: u32) -> i32;
    // xSemaphoreGive(h) → xQueueGenericSend(h, NULL, 0, queueSEND_TO_BACK)
    pub fn xQueueGenericSend(
        queue: *mut c_void,
        item: *const c_void,
        ticks_to_wait: u32,
        copy_position: i32,
    ) -> i32;

    // vSemaphoreDelete(h) → vQueueDelete(h)
    pub fn vQueueDelete(queue: *mut c_void);
}

/// Wrapper: xSemaphoreCreateRecursiveMutex()
#[inline]
pub fn create_recursive_mutex() -> *mut c_void {
    unsafe { xQueueCreateMutex(QUEUE_TYPE_RECURSIVE_MUTEX) }
}

/// Wrapper: xSemaphoreCreateMutex() (non-recursive, for condvar)
#[inline]
pub fn create_mutex() -> *mut c_void {
    unsafe { xQueueCreateMutex(QUEUE_TYPE_MUTEX) }
}

/// Wrapper: xSemaphoreTakeRecursive(h, t)
#[inline]
pub fn take_recursive(mutex: *mut c_void, ticks: u32) -> i32 {
    unsafe { xQueueTakeMutexRecursive(mutex, ticks) }
}

/// Wrapper: xSemaphoreGiveRecursive(h)
#[inline]
pub fn give_recursive(mutex: *mut c_void) -> i32 {
    unsafe { xQueueGiveMutexRecursive(mutex) }
}

/// Wrapper: xSemaphoreCreateCounting(max, initial)
#[inline]
pub fn create_counting_semaphore(max: u32, initial: u32) -> *mut c_void {
    unsafe { xQueueCreateCountingSemaphore(max, initial) }
}

/// Wrapper: xSemaphoreTake(h, t)
#[inline]
pub fn semaphore_take(sem: *mut c_void, ticks: u32) -> i32 {
    unsafe { xQueueSemaphoreTake(sem, ticks) }
}

/// Wrapper: xSemaphoreGive(h)
#[inline]
pub fn semaphore_give(sem: *mut c_void) -> i32 {
    // xSemaphoreGive expands to xQueueGenericSend(h, NULL, 0, queueSEND_TO_BACK=0)
    unsafe { xQueueGenericSend(sem, core::ptr::null(), 0, 0) }
}

/// Wrapper: vSemaphoreDelete(h)
#[inline]
pub fn semaphore_delete(sem: *mut c_void) {
    unsafe { vQueueDelete(sem) }
}

// -- Event group wrappers --

#[inline]
pub fn event_group_create() -> *mut c_void {
    unsafe { xEventGroupCreate() }
}

#[inline]
pub fn event_group_set_bits(group: *mut c_void, bits: u32) -> u32 {
    unsafe { xEventGroupSetBits(group, bits) }
}

/// pdFALSE = 0, portMAX_DELAY = 0xFFFFFFFF
#[inline]
pub fn event_group_wait_bits(group: *mut c_void, bits: u32, ticks: u32) -> u32 {
    unsafe { xEventGroupWaitBits(group, bits, 0, 0, ticks) }
}

#[inline]
pub fn event_group_delete(group: *mut c_void) {
    unsafe { vEventGroupDelete(group) }
}

// -- Critical section wrappers --

#[inline]
pub fn enter_critical() {
    unsafe { vPortEnterCritical() }
}

#[inline]
pub fn exit_critical() {
    unsafe { vPortExitCritical() }
}

// -- Task wrappers --

#[inline]
pub fn task_suspend_current() {
    unsafe { vTaskSuspend(core::ptr::null_mut()) }
}
