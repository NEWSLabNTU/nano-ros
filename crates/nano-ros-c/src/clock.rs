//! Clock API for nano-ros C API.
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
pub struct nano_ros_time_t {
    /// Seconds component
    pub sec: i32,
    /// Nanoseconds component (0 to 999,999,999)
    pub nanosec: u32,
}

/// Duration representation compatible with builtin_interfaces/msg/Duration.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct nano_ros_duration_t {
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
pub enum nano_ros_clock_type_t {
    /// Uninitialized clock
    NANO_ROS_CLOCK_UNINITIALIZED = 0,
    /// ROS time - follows /clock topic if available, otherwise system time
    NANO_ROS_CLOCK_ROS_TIME = 1,
    /// System time - wall clock time from the operating system
    NANO_ROS_CLOCK_SYSTEM_TIME = 2,
    /// Steady time - monotonic clock, not affected by system time changes
    NANO_ROS_CLOCK_STEADY_TIME = 3,
}

/// Clock state enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_clock_state_t {
    /// Not initialized
    NANO_ROS_CLOCK_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_CLOCK_STATE_READY = 1,
    /// Shutdown
    NANO_ROS_CLOCK_STATE_SHUTDOWN = 2,
}

/// Clock structure.
#[repr(C)]
pub struct nano_ros_clock_t {
    /// Clock type
    pub r#type: nano_ros_clock_type_t,
    /// Current state
    pub state: nano_ros_clock_state_t,
    /// Internal: steady clock epoch (nanoseconds since process start)
    pub _steady_epoch_ns: u64,
}

impl Default for nano_ros_clock_t {
    fn default() -> Self {
        Self {
            r#type: nano_ros_clock_type_t::NANO_ROS_CLOCK_UNINITIALIZED,
            state: nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_UNINITIALIZED,
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
pub extern "C" fn nano_ros_clock_get_zero_initialized() -> nano_ros_clock_t {
    nano_ros_clock_t::default()
}

/// Initialize a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_init(
    clock: *mut nano_ros_clock_t,
    clock_type: nano_ros_clock_type_t,
) -> nano_ros_ret_t {
    if clock.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    // Validate clock type
    match clock_type {
        nano_ros_clock_type_t::NANO_ROS_CLOCK_ROS_TIME
        | nano_ros_clock_type_t::NANO_ROS_CLOCK_SYSTEM_TIME
        | nano_ros_clock_type_t::NANO_ROS_CLOCK_STEADY_TIME => {}
        _ => return NANO_ROS_RET_INVALID_ARGUMENT,
    }

    let clock = &mut *clock;

    // Check if already initialized
    if clock.state != nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_UNINITIALIZED {
        return NANO_ROS_RET_ALREADY_EXISTS;
    }

    clock.r#type = clock_type;
    clock.state = nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_READY;

    // For steady time, record the epoch
    if clock_type == nano_ros_clock_type_t::NANO_ROS_CLOCK_STEADY_TIME {
        clock._steady_epoch_ns = get_steady_time_ns();
    }

    NANO_ROS_RET_OK
}

/// Get the current time from a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_get_now(
    clock: *const nano_ros_clock_t,
    time_out: *mut nano_ros_time_t,
) -> nano_ros_ret_t {
    if clock.is_null() || time_out.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        nano_ros_clock_type_t::NANO_ROS_CLOCK_ROS_TIME => {
            // ROS time: for now, same as system time
            // Full implementation would check for /clock topic override
            get_system_time_ns()
        }
        nano_ros_clock_type_t::NANO_ROS_CLOCK_SYSTEM_TIME => get_system_time_ns(),
        nano_ros_clock_type_t::NANO_ROS_CLOCK_STEADY_TIME => {
            // Steady time relative to epoch
            get_steady_time_ns() as i64
        }
        _ => return NANO_ROS_RET_ERROR,
    };

    *time_out = nano_ros_time_from_nanoseconds(nanos);
    NANO_ROS_RET_OK
}

/// Get the current time from a clock as nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_get_now_ns(
    clock: *const nano_ros_clock_t,
    nanoseconds: *mut i64,
) -> nano_ros_ret_t {
    if clock.is_null() || nanoseconds.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let clock = &*clock;

    if clock.state != nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    let nanos = match clock.r#type {
        nano_ros_clock_type_t::NANO_ROS_CLOCK_ROS_TIME => get_system_time_ns(),
        nano_ros_clock_type_t::NANO_ROS_CLOCK_SYSTEM_TIME => get_system_time_ns(),
        nano_ros_clock_type_t::NANO_ROS_CLOCK_STEADY_TIME => get_steady_time_ns() as i64,
        _ => return NANO_ROS_RET_ERROR,
    };

    *nanoseconds = nanos;
    NANO_ROS_RET_OK
}

/// Check if a clock is valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_is_valid(clock: *const nano_ros_clock_t) -> bool {
    if clock.is_null() {
        return false;
    }

    let clock = &*clock;
    clock.state == nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_READY
}

/// Get the clock type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_get_type(
    clock: *const nano_ros_clock_t,
) -> nano_ros_clock_type_t {
    if clock.is_null() {
        return nano_ros_clock_type_t::NANO_ROS_CLOCK_UNINITIALIZED;
    }

    (*clock).r#type
}

/// Finalize a clock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_clock_fini(clock: *mut nano_ros_clock_t) -> nano_ros_ret_t {
    if clock.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let clock = &mut *clock;

    if clock.state == nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_UNINITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    clock.state = nano_ros_clock_state_t::NANO_ROS_CLOCK_STATE_SHUTDOWN;
    clock.r#type = nano_ros_clock_type_t::NANO_ROS_CLOCK_UNINITIALIZED;
    clock._steady_epoch_ns = 0;

    NANO_ROS_RET_OK
}

// ============================================================================
// Time Utility Functions
// ============================================================================

/// Convert nanoseconds to a nano_ros_time_t structure.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_time_from_nanoseconds(nanoseconds: i64) -> nano_ros_time_t {
    let sec = (nanoseconds / NANOS_PER_SEC as i64) as i32;
    let nanosec = (nanoseconds.unsigned_abs() % NANOS_PER_SEC) as u32;

    nano_ros_time_t { sec, nanosec }
}

/// Convert a nano_ros_time_t structure to nanoseconds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_time_to_nanoseconds(time: *const nano_ros_time_t) -> i64 {
    if time.is_null() {
        return 0;
    }

    let time = &*time;
    (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64)
}

/// Add a duration to a time.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_time_add(
    time: nano_ros_time_t,
    duration: nano_ros_duration_t,
) -> nano_ros_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    nano_ros_time_from_nanoseconds(time_ns + duration_ns)
}

/// Subtract a duration from a time.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_time_sub(
    time: nano_ros_time_t,
    duration: nano_ros_duration_t,
) -> nano_ros_time_t {
    let time_ns = (time.sec as i64) * (NANOS_PER_SEC as i64) + (time.nanosec as i64);
    let duration_ns = (duration.sec as i64) * (NANOS_PER_SEC as i64) + (duration.nanosec as i64);

    nano_ros_time_from_nanoseconds(time_ns - duration_ns)
}

/// Compare two times.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_time_compare(a: nano_ros_time_t, b: nano_ros_time_t) -> c_int {
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
