//! Timer API for nros C API.
//!
//! Timers provide periodic callbacks for time-based operations.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::error::*;
use crate::support::{nros_support_state_t, nros_support_t};

/// Timer callback function type.
///
/// # Parameters
/// * `timer` - Pointer to the timer that triggered
/// * `context` - User-provided context pointer
pub type nros_timer_callback_t =
    Option<unsafe extern "C" fn(timer: *mut nros_timer_t, context: *mut c_void)>;

/// Timer state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_timer_state_t {
    /// Not initialized
    NROS_TIMER_STATE_UNINITIALIZED = 0,
    /// Initialized and running
    NROS_TIMER_STATE_RUNNING = 1,
    /// Initialized but canceled
    NROS_TIMER_STATE_CANCELED = 2,
    /// Shutdown
    NROS_TIMER_STATE_SHUTDOWN = 3,
}

/// Timer structure.
#[repr(C)]
pub struct nros_timer_t {
    /// Current state
    pub state: nros_timer_state_t,
    /// Period in nanoseconds
    pub period_ns: u64,
    /// Last trigger time in nanoseconds
    pub last_call_time_ns: u64,
    /// User callback function
    callback: nros_timer_callback_t,
    /// User context pointer
    context: *mut c_void,
    /// Pointer to parent support context
    support: *const nros_support_t,
    /// Handle ID from executor registration (usize::MAX = not registered)
    handle_id: usize,
    /// Opaque pointer to internal executor (set by nros_executor_add_timer)
    _executor: *mut c_void,
}

impl Default for nros_timer_t {
    fn default() -> Self {
        Self {
            state: nros_timer_state_t::NROS_TIMER_STATE_UNINITIALIZED,
            period_ns: 0,
            last_call_time_ns: 0,
            callback: None,
            context: ptr::null_mut(),
            support: ptr::null(),
            handle_id: usize::MAX,
            _executor: ptr::null_mut(),
        }
    }
}

// Internal helper methods for executor
impl nros_timer_t {
    /// Get the callback function
    pub(crate) fn get_callback(&self) -> nros_timer_callback_t {
        self.callback
    }

    /// Get the user context
    pub(crate) fn get_context(&self) -> *mut c_void {
        self.context
    }

    /// Set the handle ID from executor registration
    pub(crate) fn set_handle_id(&mut self, id: nros_node::HandleId) {
        self.handle_id = id.0;
    }

    /// Set the executor pointer (called by nros_executor_add_timer)
    pub(crate) fn set_executor_ptr(&mut self, executor: *mut c_void) {
        self._executor = executor;
    }
}

/// Get a zero-initialized timer.
#[unsafe(no_mangle)]
pub extern "C" fn nros_timer_get_zero_initialized() -> nros_timer_t {
    nros_timer_t::default()
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any required pointer is NULL or period is 0
/// * `NROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All required pointers must be valid
/// * `callback` must be a valid function pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_init(
    timer: *mut nros_timer_t,
    support: *const nros_support_t,
    period_ns: u64,
    callback: nros_timer_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    // Validate arguments
    if timer.is_null() || support.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    if callback.is_none() || period_ns == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;
    let support_ref = &*support;

    // Check if timer is already initialized
    if timer.state != nros_timer_state_t::NROS_TIMER_STATE_UNINITIALIZED {
        return NROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    timer.period_ns = period_ns;
    timer.callback = callback;
    timer.context = context;
    timer.support = support;
    timer.last_call_time_ns = 0;
    timer.state = nros_timer_state_t::NROS_TIMER_STATE_RUNNING;

    NROS_RET_OK
}

/// Cancel a timer.
///
/// A canceled timer will not fire, but can be reset to start again.
/// If registered with an executor, forwards to the executor's cancel_timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_cancel(timer: *mut nros_timer_t) -> nros_ret_t {
    if timer.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    match timer.state {
        nros_timer_state_t::NROS_TIMER_STATE_RUNNING => {
            // Forward to executor if registered
            #[cfg(feature = "alloc")]
            if !timer._executor.is_null() && timer.handle_id != usize::MAX {
                let exec = &mut *(timer._executor as *mut crate::executor::CExecutor);
                let _ = exec.cancel_timer(nros_node::HandleId(timer.handle_id));
            }

            timer.state = nros_timer_state_t::NROS_TIMER_STATE_CANCELED;
            NROS_RET_OK
        }
        nros_timer_state_t::NROS_TIMER_STATE_CANCELED => {
            // Already canceled
            NROS_RET_OK
        }
        _ => NROS_RET_NOT_INIT,
    }
}

/// Reset a timer.
///
/// This resets the timer's last call time and starts it running again
/// if it was canceled. If registered with an executor, forwards to the
/// executor's reset_timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_reset(timer: *mut nros_timer_t) -> nros_ret_t {
    if timer.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    match timer.state {
        nros_timer_state_t::NROS_TIMER_STATE_RUNNING
        | nros_timer_state_t::NROS_TIMER_STATE_CANCELED => {
            // Forward to executor if registered
            #[cfg(feature = "alloc")]
            if !timer._executor.is_null() && timer.handle_id != usize::MAX {
                let exec = &mut *(timer._executor as *mut crate::executor::CExecutor);
                let _ = exec.reset_timer(nros_node::HandleId(timer.handle_id));
            }

            timer.last_call_time_ns = 0;
            timer.state = nros_timer_state_t::NROS_TIMER_STATE_RUNNING;
            NROS_RET_OK
        }
        _ => NROS_RET_NOT_INIT,
    }
}

/// Finalize a timer.
///
/// # Parameters
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_fini(timer: *mut nros_timer_t) -> nros_ret_t {
    if timer.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let timer = &mut *timer;

    if timer.state == nros_timer_state_t::NROS_TIMER_STATE_UNINITIALIZED
        || timer.state == nros_timer_state_t::NROS_TIMER_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    timer.callback = None;
    timer.context = ptr::null_mut();
    timer.support = ptr::null();
    timer.handle_id = usize::MAX;
    timer._executor = ptr::null_mut();
    timer.state = nros_timer_state_t::NROS_TIMER_STATE_SHUTDOWN;

    NROS_RET_OK
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
pub unsafe extern "C" fn nros_timer_is_ready(
    timer: *const nros_timer_t,
    current_time_ns: u64,
) -> c_int {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;

    if timer.state != nros_timer_state_t::NROS_TIMER_STATE_RUNNING {
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
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if timer is NULL
/// * `NROS_RET_NOT_INIT` if not initialized or not running
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_call(
    timer: *mut nros_timer_t,
    current_time_ns: u64,
) -> nros_ret_t {
    if timer.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let timer_ref = &mut *timer;

    if timer_ref.state != nros_timer_state_t::NROS_TIMER_STATE_RUNNING {
        return NROS_RET_NOT_INIT;
    }

    // Update last call time
    timer_ref.last_call_time_ns = current_time_ns;

    // Call the callback
    if let Some(cb) = timer_ref.callback {
        cb(timer, timer_ref.context);
    }

    NROS_RET_OK
}

/// Check if timer is valid (initialized and not shutdown).
///
/// # Parameters
/// * `timer` - Pointer to a timer
///
/// # Returns
/// * Non-zero if valid, 0 if invalid or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_timer_is_valid(timer: *const nros_timer_t) -> c_int {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;
    match timer.state {
        nros_timer_state_t::NROS_TIMER_STATE_RUNNING
        | nros_timer_state_t::NROS_TIMER_STATE_CANCELED => 1,
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
pub unsafe extern "C" fn nros_timer_get_period(timer: *const nros_timer_t) -> u64 {
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
pub unsafe extern "C" fn nros_timer_get_time_until_next_call(
    timer: *const nros_timer_t,
    current_time_ns: u64,
) -> u64 {
    if timer.is_null() {
        return 0;
    }

    let timer = &*timer;

    if timer.state != nros_timer_state_t::NROS_TIMER_STATE_RUNNING {
        return 0;
    }

    let elapsed = current_time_ns.saturating_sub(timer.last_call_time_ns);
    timer.period_ns.saturating_sub(elapsed)
}
