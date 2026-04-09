//! FreeRTOS C API declarations.
//!
//! These are resolved at link time from the FreeRTOS kernel library
//! linked by the board crate. No Rust-side FreeRTOS binding crate needed.

use core::ffi::c_void;

unsafe extern "C" {
    // Clock
    pub fn xTaskGetTickCount() -> u32;

    // Memory
    pub fn pvPortMalloc(size: usize) -> *mut c_void;
    pub fn vPortFree(ptr: *mut c_void);

    // Sleep
    pub fn vTaskDelay(ticks: u32);

    // Tasks
    pub fn xTaskCreate(
        task_code: unsafe extern "C" fn(*mut c_void),
        name: *const core::ffi::c_char,
        stack_depth: u32,
        parameters: *mut c_void,
        priority: u32,
        task_handle: *mut *mut c_void,
    ) -> i32;
    pub fn vTaskDelete(task_handle: *mut c_void);

    // Semaphores / Mutexes
    pub fn xSemaphoreCreateRecursiveMutex() -> *mut c_void;
    pub fn xSemaphoreCreateCounting(max_count: u32, initial_count: u32) -> *mut c_void;
    pub fn xSemaphoreTakeRecursive(semaphore: *mut c_void, ticks_to_wait: u32) -> i32;
    pub fn xSemaphoreGiveRecursive(semaphore: *mut c_void) -> i32;
    pub fn xSemaphoreTake(semaphore: *mut c_void, ticks_to_wait: u32) -> i32;
    pub fn xSemaphoreGive(semaphore: *mut c_void) -> i32;
    pub fn vSemaphoreDelete(semaphore: *mut c_void);
}
