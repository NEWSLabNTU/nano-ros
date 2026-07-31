//! Clock API for nros C API.
//!
//! Provides time sources for ROS 2 compatible timing operations.

use core::ffi::c_int;

use crate::error::*;

// ============================================================================
// Time Types
// ============================================================================

/// Time representation compatible with builtin_interfaces/msg/Time.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct nros_time_t {
    /// Seconds component
    pub sec: i32,
    /// Nanoseconds component (0 to 999,999,999)
    pub nanosec: u32,
}

/// Duration representation compatible with builtin_interfaces/msg/Duration.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct nros_duration_t {
    /// Seconds component (can be negative)
    pub sec: i32,
    /// Nanoseconds component (0 to 999,999,999)
    pub nanosec: u32,
}

// ============================================================================
// Clock Types
// ============================================================================

/// Clock type enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_clock_type_t {
    /// Uninitialized clock
    NROS_CLOCK_UNINITIALIZED = 0,
    /// ROS time - follows /clock topic if available, otherwise system time
    NROS_CLOCK_ROS_TIME = 1,
    /// System time - wall clock time from the operating system
    NROS_CLOCK_SYSTEM_TIME = 2,
    /// Steady time - monotonic clock, not affected by system time changes
    NROS_CLOCK_STEADY_TIME = 3,
}

/// Clock state enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_clock_state_t {
    /// Not initialized
    NROS_CLOCK_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_CLOCK_STATE_READY = 1,
    /// Shutdown
    NROS_CLOCK_STATE_SHUTDOWN = 2,
}

/// Clock structure.
#[repr(C)]
pub struct nros_clock_t {
    /// Clock type
    pub r#type: nros_clock_type_t,
    /// Current state
    pub state: nros_clock_state_t,
    /// Internal: steady clock epoch (nanoseconds since process start)
    pub _steady_epoch_ns: u64,
}

impl Default for nros_clock_t {
    fn default() -> Self {
        Self {
            r#type: nros_clock_type_t::NROS_CLOCK_UNINITIALIZED,
            state: nros_clock_state_t::NROS_CLOCK_STATE_UNINITIALIZED,
            _steady_epoch_ns: 0,
        }
    }
}

// ============================================================================
// Platform-specific time functions
// ============================================================================

use crate::platform;

/// Nanoseconds per second constant
const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Get system time in nanoseconds since Unix epoch.
fn get_system_time_ns() -> i64 {
    platform::get_system_time_ns()
}

/// Get steady (monotonic) time in nanoseconds.
fn get_steady_time_ns() -> u64 {
    platform::get_time_ns()
}

// ============================================================================
// Clock Functions
// ============================================================================

/// Get a zero-initialized clock.
#[unsafe(no_mangle)]
pub extern "C" fn nros_clock_get_zero_initialized() -> nros_clock_t {
    nros_clock_t::default()
}

/// Initialize a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_init(
    clock: *mut nros_clock_t,
    clock_type: nros_clock_type_t,
) -> nros_ret_t {
    if clock.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    // Validate clock type
    match clock_type {
        nros_clock_type_t::NROS_CLOCK_ROS_TIME
        | nros_clock_type_t::NROS_CLOCK_SYSTEM_TIME
        | nros_clock_type_t::NROS_CLOCK_STEADY_TIME => {}
        _ => return NROS_RET_INVALID_ARGUMENT,
    }

    let clock = &mut *clock;

    // Check if already initialized
    if clock.state != nros_clock_state_t::NROS_CLOCK_STATE_UNINITIALIZED {
        return NROS_RET_ALREADY_EXISTS;
    }

    clock.r#type = clock_type;
    clock.state = nros_clock_state_t::NROS_CLOCK_STATE_READY;

    // For steady time, record the epoch
    if clock_type == nros_clock_type_t::NROS_CLOCK_STEADY_TIME {
        clock._steady_epoch_ns = get_steady_time_ns();
    }

    NROS_RET_OK
}

/// Get the current time from a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_get_now(
    clock: *const nros_clock_t,
    time_out: *mut nros_time_t,
) -> nros_ret_t {
    if clock.is_null() || time_out.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != nros_clock_state_t::NROS_CLOCK_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        nros_clock_type_t::NROS_CLOCK_ROS_TIME => {
            // ROS time: for now, same as system time
            // Full implementation would check for /clock topic override
            get_system_time_ns()
        }
        nros_clock_type_t::NROS_CLOCK_SYSTEM_TIME => get_system_time_ns(),
        nros_clock_type_t::NROS_CLOCK_STEADY_TIME => {
            // Steady time relative to epoch
            get_steady_time_ns() as i64
        }
        _ => return NROS_RET_ERROR,
    };

    *time_out = nros_time_from_nanoseconds(nanos);
    NROS_RET_OK
}

/// Get the current time from a clock as nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_get_now_ns(
    clock: *const nros_clock_t,
    nanoseconds: *mut i64,
) -> nros_ret_t {
    if clock.is_null() || nanoseconds.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != nros_clock_state_t::NROS_CLOCK_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        nros_clock_type_t::NROS_CLOCK_ROS_TIME => get_system_time_ns(),
        nros_clock_type_t::NROS_CLOCK_SYSTEM_TIME => get_system_time_ns(),
        nros_clock_type_t::NROS_CLOCK_STEADY_TIME => get_steady_time_ns() as i64,
        _ => return NROS_RET_ERROR,
    };

    *nanoseconds = nanos;
    NROS_RET_OK
}

/// Check if a clock is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_is_valid(clock: *const nros_clock_t) -> bool {
    if clock.is_null() {
        return false;
    }

    let clock = &*clock;
    clock.state == nros_clock_state_t::NROS_CLOCK_STATE_READY
}

/// Get the clock type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_get_type(clock: *const nros_clock_t) -> nros_clock_type_t {
    if clock.is_null() {
        return nros_clock_type_t::NROS_CLOCK_UNINITIALIZED;
    }

    (*clock).r#type
}

/// Finalize a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_clock_fini(clock: *mut nros_clock_t) -> nros_ret_t {
    if clock.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let clock = &mut *clock;

    if clock.state == nros_clock_state_t::NROS_CLOCK_STATE_UNINITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    clock.state = nros_clock_state_t::NROS_CLOCK_STATE_SHUTDOWN;
    clock.r#type = nros_clock_type_t::NROS_CLOCK_UNINITIALIZED;
    clock._steady_epoch_ns = 0;

    NROS_RET_OK
}

// ============================================================================
// Time Utility Functions
// ============================================================================

/// Convert nanoseconds to a nros_time_t structure.
#[unsafe(no_mangle)]
pub extern "C" fn nros_time_from_nanoseconds(nanoseconds: i64) -> nros_time_t {
    let sec = (nanoseconds / NANOS_PER_SEC as i64) as i32;
    let nanosec = (nanoseconds.unsigned_abs() % NANOS_PER_SEC) as u32;

    nros_time_t { sec, nanosec }
}

/// Convert a nros_time_t structure to nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_time_to_nanoseconds(time: *const nros_time_t) -> i64 {
    if time.is_null() {
        return 0;
    }

    let time = &*time;
    (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64)
}

/// Add a duration to a time.
#[unsafe(no_mangle)]
pub extern "C" fn nros_time_add(time: nros_time_t, duration: nros_duration_t) -> nros_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    nros_time_from_nanoseconds(time_ns + duration_ns)
}

/// Subtract a duration from a time.
#[unsafe(no_mangle)]
pub extern "C" fn nros_time_sub(time: nros_time_t, duration: nros_duration_t) -> nros_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    nros_time_from_nanoseconds(time_ns - duration_ns)
}

/// Compare two times.
#[unsafe(no_mangle)]
pub extern "C" fn nros_time_compare(a: nros_time_t, b: nros_time_t) -> c_int {
    if a.sec < b.sec {
        -1
    } else if a.sec > b.sec {
        1
    } else if a.nanosec < b.nanosec {
        -1
    } else if a.nanosec > b.nanosec {
        1
    } else {
        0
    }
}
