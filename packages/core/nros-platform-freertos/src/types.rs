//! `#[repr(C)]` types matching zenoh-pico's FreeRTOS platform structs.
//!
//! These must match the layouts in `zenoh-pico/include/zenoh-pico/system/
//! platform/freertos/lwip.h` (with `configSUPPORT_STATIC_ALLOCATION = 0`).

use core::ffi::c_void;

/// Matches `_z_task_t` on FreeRTOS (16 bytes on ARM32).
#[repr(C)]
pub struct ZTask {
    pub handle: *mut c_void,
    pub join_event: *mut c_void,
    pub fun: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    pub arg: *mut c_void,
}

/// Matches `z_task_attr_t` on FreeRTOS (12 bytes on ARM32).
#[repr(C)]
pub struct ZTaskAttr {
    pub name: *const u8,
    pub priority: u32,
    pub stack_depth: usize,
}

/// Matches `_z_mutex_t` on FreeRTOS (4 bytes on ARM32).
#[repr(C)]
pub struct ZMutex {
    pub handle: *mut c_void,
}

/// Matches `_z_condvar_t` on FreeRTOS (12 bytes on ARM32).
#[repr(C)]
pub struct ZCondvar {
    pub mutex: *mut c_void,
    pub sem: *mut c_void,
    pub waiters: i32,
}
