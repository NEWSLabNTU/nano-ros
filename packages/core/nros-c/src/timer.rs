//! Timer API for nros C API.
//!
//! Timers provide periodic callbacks for time-based operations.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::error::*;
use crate::support::{nano_ros_support_state_t, nano_ros_support_t};

/// Timer callback function type.
///
/// # Parameters
/// * `timer` - Pointer to the timer that triggered
/// * `context` - User-provided context pointer
pub type nano_ros_timer_callback_t =
    Option<unsafe extern "C" fn(timer: *mut nano_ros_timer_t, context: *mut c_void)>;

/// Timer state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_timer_state_t {
    /// Not initialized
    NANO_ROS_TIMER_STATE_UNINITIALIZED = 0,
    /// Initialized and running
    NANO_ROS_TIMER_STATE_RUNNING = 1,
    /// Initialized but canceled
    NANO_ROS_TIMER_STATE_CANCELED = 2,
    /// Shutdown
    NANO_ROS_TIMER_STATE_SHUTDOWN = 3,
}

/// Timer structure.
#[repr(C)]
pub struct nano_ros_timer_t {
    /// Current state
    pub state: nano_ros_timer_state_t,
    /// Period in nanoseconds
    pub period_ns: u64,
    /// Last trigger time in nanoseconds
    pub last_call_time_ns: u64,
    /// User callback function
    callback: nano_ros_timer_callback_t,
    /// User context pointer
    context: *mut c_void,
    /// Pointer to parent support context
    support: *const nano_ros_support_t,
}

impl Default for nano_ros_timer_t {
    fn default() -> Self {
        Self {
            state: nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_UNINITIALIZED,
            period_ns: 0,
            last_call_time_ns: 0,
            callback: None,
            context: ptr::null_mut(),
            support: ptr::null(),
        }
    }
}

/// Get a zero-initialized timer.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_timer_get_zero_initialized() -> nano_ros_timer_t {
    nano_ros_timer_t::default()
}

/// Initialize a timer.
///
/// # Parameters
/// * `timer` - Pointer to a zero-initialized timer
/// * `support` - Pointer to an initialized support context
/// * `period_ns` - Timer period in nanoseconds
/// * `callback` - Callback function to invoke when timer fires
/// * `context` - User context pointer passed to callback (can be NULL)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any required pointer is NULL or period is 0
/// * `NANO_ROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All required pointers must be valid
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_init(
    timer: *mut nano_ros_timer_t,
    support: *const nano_ros_support_t,
    period_ns: u64,
    callback: nano_ros_timer_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    // Validate arguments
    if timer.is_null() || support.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    if callback.is_none() || period_ns == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;
    let support_ref = &*support;

    // Check if timer is already initialized
    if timer.state != nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_UNINITIALIZED {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    timer.period_ns = period_ns;
    timer.callback = callback;
    timer.context = context;
    timer.support = support;
    timer.last_call_time_ns = 0;
    timer.state = nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING;

    NANO_ROS_RET_OK
}

/// Cancel a timer.
///
/// A canceled timer will not fire, but can be reset to start again.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_cancel(timer: *mut nano_ros_timer_t) -> nano_ros_ret_t {
    if timer.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    match timer.state {
        nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING => {
            timer.state = nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_CANCELED;
            NANO_ROS_RET_OK
        }
        nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_CANCELED => {
            // Already canceled
            NANO_ROS_RET_OK
        }
        _ => NANO_ROS_RET_NOT_INIT,
    }
}

/// Reset a timer.
///
/// This resets the timer's last call time and starts it running again
/// if it was canceled.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_reset(timer: *mut nano_ros_timer_t) -> nano_ros_ret_t {
    if timer.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    match timer.state {
        nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING
        | nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_CANCELED => {
            timer.last_call_time_ns = 0;
            timer.state = nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING;
            NANO_ROS_RET_OK
        }
        _ => NANO_ROS_RET_NOT_INIT,
    }
}

/// Finalize a timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_fini(timer: *mut nano_ros_timer_t) -> nano_ros_ret_t {
    if timer.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    if timer.state == nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_UNINITIALIZED
        || timer.state == nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_SHUTDOWN
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    timer.callback = None;
    timer.context = ptr::null_mut();
    timer.support = ptr::null();
    timer.state = nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Check if timer is ready to fire.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
/// * `current_time_ns` - Current time in nanoseconds
///
/// # Returns
/// * Non-zero if timer is ready, 0 otherwise
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_is_ready(
    timer: *const nano_ros_timer_t,
    current_time_ns: u64,
) -> c_int {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;

    if timer.state != nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING {
        return 0;
    }

    let elapsed = current_time_ns.saturating_sub(timer.last_call_time_ns);
    if elapsed >= timer.period_ns { 1 } else { 0 }
}

/// Call the timer callback and update last call time.
///
/// This is called by the executor when the timer is ready.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
/// * `current_time_ns` - Current time in nanoseconds
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized or not running
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_call(
    timer: *mut nano_ros_timer_t,
    current_time_ns: u64,
) -> nano_ros_ret_t {
    if timer.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let timer_ref = &mut *timer;

    if timer_ref.state != nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Update last call time
    timer_ref.last_call_time_ns = current_time_ns;

    // Call the callback
    if let Some(cb) = timer_ref.callback {
        cb(timer, timer_ref.context);
    }

    NANO_ROS_RET_OK
}

/// Check if timer is valid (initialized and not shutdown).
///
/// # Parameters
/// * `timer` - Pointer to a timer
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_is_valid(timer: *const nano_ros_timer_t) -> c_int {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;
    match timer.state {
        nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING
        | nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_CANCELED => 1,
        _ => 0,
    }
}

/// Get the timer period in nanoseconds.
///
/// # Parameters
/// * `timer` - Pointer to a timer
///
/// # Returns
/// * Period in nanoseconds, or 0 if invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_get_period(timer: *const nano_ros_timer_t) -> u64 {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;
    timer.period_ns
}

/// Get the time until next timer firing.
///
/// # Parameters
/// * `timer` - Pointer to a timer
/// * `current_time_ns` - Current time in nanoseconds
///
/// # Returns
/// * Time until next firing in nanoseconds, or 0 if ready now or invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_timer_get_time_until_next_call(
    timer: *const nano_ros_timer_t,
    current_time_ns: u64,
) -> u64 {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;

    if timer.state != nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING {
        return 0;
    }

    let elapsed = current_time_ns.saturating_sub(timer.last_call_time_ns);
    timer.period_ns.saturating_sub(elapsed)
}
