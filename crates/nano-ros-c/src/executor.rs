//! Executor API for nano-ros C API.
//!
//! The executor manages callbacks for subscriptions, timers, and services,
//! providing deterministic execution in a user-defined order.

use core::ffi::c_int;
use core::ptr;

use crate::error::*;
use crate::guard_condition::{nano_ros_guard_condition_state_t, nano_ros_guard_condition_t};
use crate::service::{nano_ros_service_state_t, nano_ros_service_t};
use crate::subscription::{nano_ros_subscription_state_t, nano_ros_subscription_t};
use crate::support::{nano_ros_support_state_t, nano_ros_support_t};
use crate::timer::{nano_ros_timer_state_t, nano_ros_timer_t};

/// Maximum number of handles in an executor
pub const NANO_ROS_EXECUTOR_MAX_HANDLES: usize = 16;

/// Callback invocation mode
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_executor_invocation_t {
    /// Only invoke callback when new data is available
    NANO_ROS_EXECUTOR_ON_NEW_DATA = 0,
    /// Always invoke callback (even with NULL data)
    NANO_ROS_EXECUTOR_ALWAYS = 1,
}

/// Executor data communication semantics
///
/// Defines when data is taken from DDS during spin operations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_executor_semantics_t {
    /// RCLCPP executor semantics: Data is taken from DDS just before
    /// the corresponding callback is called.
    NANO_ROS_SEMANTICS_RCLCPP_EXECUTOR = 0,
    /// Logical Execution Time (LET) semantics: At one sampling point,
    /// new data of all ready subscriptions are taken from DDS.
    /// During sequential processing, the data from that sampling point
    /// is used. New data arriving after the sampling point is not
    /// considered until the next spin iteration.
    NANO_ROS_SEMANTICS_LOGICAL_EXECUTION_TIME = 1,
}

/// Handle type for executor
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_executor_handle_type_t {
    /// No handle (empty slot)
    NANO_ROS_EXECUTOR_HANDLE_NONE = 0,
    /// Subscription handle
    NANO_ROS_EXECUTOR_HANDLE_SUBSCRIPTION = 1,
    /// Timer handle
    NANO_ROS_EXECUTOR_HANDLE_TIMER = 2,
    /// Service handle
    NANO_ROS_EXECUTOR_HANDLE_SERVICE = 3,
    /// Client handle
    NANO_ROS_EXECUTOR_HANDLE_CLIENT = 4,
    /// Guard condition handle
    NANO_ROS_EXECUTOR_HANDLE_GUARD_CONDITION = 5,
}

/// Executor handle (union-like structure)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nano_ros_executor_handle_t {
    /// Handle type
    pub handle_type: nano_ros_executor_handle_type_t,
    /// Invocation mode (for subscriptions)
    pub invocation: nano_ros_executor_invocation_t,
    /// Handle pointer (type depends on handle_type)
    pub handle: *mut core::ffi::c_void,
    /// Flag indicating if handle has new data ready
    pub data_ready: bool,
}

impl Default for nano_ros_executor_handle_t {
    fn default() -> Self {
        Self {
            handle_type: nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_NONE,
            invocation: nano_ros_executor_invocation_t::NANO_ROS_EXECUTOR_ON_NEW_DATA,
            handle: ptr::null_mut(),
            data_ready: false,
        }
    }
}

/// Executor state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_executor_state_t {
    /// Not initialized
    NANO_ROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_EXECUTOR_STATE_INITIALIZED = 1,
    /// Currently spinning
    NANO_ROS_EXECUTOR_STATE_SPINNING = 2,
    /// Shutdown
    NANO_ROS_EXECUTOR_STATE_SHUTDOWN = 3,
}

/// Executor structure.
///
/// The executor manages a fixed array of handles and processes them
/// in the order they were added.
#[repr(C)]
pub struct nano_ros_executor_t {
    /// Current state
    pub state: nano_ros_executor_state_t,
    /// Handle array
    handles: [nano_ros_executor_handle_t; NANO_ROS_EXECUTOR_MAX_HANDLES],
    /// Number of handles in use
    handle_count: usize,
    /// Maximum handles (configured at init)
    max_handles: usize,
    /// Timeout in nanoseconds for spin_some
    pub timeout_ns: u64,
    /// Data communication semantics
    pub semantics: nano_ros_executor_semantics_t,
    /// Pointer to support context
    support: *const nano_ros_support_t,
}

impl Default for nano_ros_executor_t {
    fn default() -> Self {
        Self {
            state: nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_UNINITIALIZED,
            handles: [nano_ros_executor_handle_t::default(); NANO_ROS_EXECUTOR_MAX_HANDLES],
            handle_count: 0,
            max_handles: NANO_ROS_EXECUTOR_MAX_HANDLES,
            timeout_ns: 100_000_000, // 100ms default
            semantics: nano_ros_executor_semantics_t::NANO_ROS_SEMANTICS_RCLCPP_EXECUTOR,
            support: ptr::null(),
        }
    }
}

/// Get a zero-initialized executor.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_executor_get_zero_initialized() -> nano_ros_executor_t {
    nano_ros_executor_t::default()
}

/// Initialize an executor.
///
/// # Parameters
/// * `executor` - Pointer to a zero-initialized executor
/// * `support` - Pointer to an initialized support context
/// * `max_handles` - Maximum number of handles (capped at NANO_ROS_EXECUTOR_MAX_HANDLES)
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL or max_handles is 0
/// * `NANO_ROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All pointers must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_init(
    executor: *mut nano_ros_executor_t,
    support: *const nano_ros_support_t,
    max_handles: usize,
) -> nano_ros_ret_t {
    if executor.is_null() || support.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    if max_handles == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let support_ref = &*support;

    // Check if executor is already initialized
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_UNINITIALIZED {
        return NANO_ROS_RET_BAD_SEQUENCE;
    }

    // Check if support is initialized
    if support_ref.state != nano_ros_support_state_t::NANO_ROS_SUPPORT_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Cap max_handles at array size
    executor.max_handles = max_handles.min(NANO_ROS_EXECUTOR_MAX_HANDLES);
    executor.handle_count = 0;
    executor.support = support;
    executor.timeout_ns = 100_000_000; // 100ms default
    executor.state = nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED;

    NANO_ROS_RET_OK
}

/// Set the executor timeout.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `timeout_ns` - Timeout in nanoseconds for spin_some
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_set_timeout(
    executor: *mut nano_ros_executor_t,
    timeout_ns: u64,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;

    if executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    executor.timeout_ns = timeout_ns;
    NANO_ROS_RET_OK
}

/// Set data communication semantics.
///
/// Controls when data is taken from DDS during spin operations.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `semantics` - The data communication semantics to use
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
/// * `NANO_ROS_RET_NOT_INIT` if executor is not initialized
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_set_semantics(
    executor: *mut nano_ros_executor_t,
    semantics: nano_ros_executor_semantics_t,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;

    if executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    executor.semantics = semantics;
    NANO_ROS_RET_OK
}

/// Add a subscription to the executor.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `subscription` - Pointer to an initialized subscription
/// * `invocation` - When to invoke the callback
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_FULL` if executor is full
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_add_subscription(
    executor: *mut nano_ros_executor_t,
    subscription: *mut nano_ros_subscription_t,
    invocation: nano_ros_executor_invocation_t,
) -> nano_ros_ret_t {
    if executor.is_null() || subscription.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let subscription_ref = &*subscription;

    // Check executor state
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check subscription state
    if subscription_ref.state
        != nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check if full
    if executor.handle_count >= executor.max_handles {
        return NANO_ROS_RET_FULL;
    }

    // Add handle
    let idx = executor.handle_count;
    executor.handles[idx] = nano_ros_executor_handle_t {
        handle_type: nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_SUBSCRIPTION,
        invocation,
        handle: subscription as *mut _,
        data_ready: false,
    };
    executor.handle_count += 1;

    NANO_ROS_RET_OK
}

/// Add a timer to the executor.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `timer` - Pointer to an initialized timer
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_FULL` if executor is full
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_add_timer(
    executor: *mut nano_ros_executor_t,
    timer: *mut nano_ros_timer_t,
) -> nano_ros_ret_t {
    if executor.is_null() || timer.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let timer_ref = &*timer;

    // Check executor state
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check timer state
    if timer_ref.state != nano_ros_timer_state_t::NANO_ROS_TIMER_STATE_RUNNING {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check if full
    if executor.handle_count >= executor.max_handles {
        return NANO_ROS_RET_FULL;
    }

    // Add handle
    let idx = executor.handle_count;
    executor.handles[idx] = nano_ros_executor_handle_t {
        handle_type: nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_TIMER,
        invocation: nano_ros_executor_invocation_t::NANO_ROS_EXECUTOR_ALWAYS,
        handle: timer as *mut _,
        data_ready: false,
    };
    executor.handle_count += 1;

    NANO_ROS_RET_OK
}

/// Add a service to the executor.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `service` - Pointer to an initialized service
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_FULL` if executor is full
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_add_service(
    executor: *mut nano_ros_executor_t,
    service: *mut nano_ros_service_t,
) -> nano_ros_ret_t {
    if executor.is_null() || service.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let service_ref = &*service;

    // Check executor state
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check service state
    if service_ref.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check if full
    if executor.handle_count >= executor.max_handles {
        return NANO_ROS_RET_FULL;
    }

    // Add handle
    let idx = executor.handle_count;
    executor.handles[idx] = nano_ros_executor_handle_t {
        handle_type: nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_SERVICE,
        invocation: nano_ros_executor_invocation_t::NANO_ROS_EXECUTOR_ON_NEW_DATA,
        handle: service as *mut _,
        data_ready: false,
    };
    executor.handle_count += 1;

    NANO_ROS_RET_OK
}

/// Add a guard condition to the executor.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `guard` - Pointer to an initialized guard condition
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if any pointer is NULL
/// * `NANO_ROS_RET_FULL` if executor is full
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_add_guard_condition(
    executor: *mut nano_ros_executor_t,
    guard: *mut nano_ros_guard_condition_t,
) -> nano_ros_ret_t {
    if executor.is_null() || guard.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let guard_ref = &*guard;

    // Check executor state
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check guard condition state
    if guard_ref.state
        != nano_ros_guard_condition_state_t::NANO_ROS_GUARD_CONDITION_STATE_INITIALIZED
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check if full
    if executor.handle_count >= executor.max_handles {
        return NANO_ROS_RET_FULL;
    }

    // Add handle
    let idx = executor.handle_count;
    executor.handles[idx] = nano_ros_executor_handle_t {
        handle_type: nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_GUARD_CONDITION,
        invocation: nano_ros_executor_invocation_t::NANO_ROS_EXECUTOR_ON_NEW_DATA,
        handle: guard as *mut _,
        data_ready: false,
    };
    executor.handle_count += 1;

    NANO_ROS_RET_OK
}

/// Maximum buffer size for subscription/service data
const MESSAGE_BUFFER_SIZE: usize = 4096;

/// Process a subscription message if one is available.
///
/// Returns true if a message was processed, false otherwise.
#[cfg(feature = "std")]
unsafe fn process_subscription(subscription: *mut nano_ros_subscription_t) -> bool {
    use nano_ros_transport::{Subscriber, ZenohSubscriber};

    let subscription_ref = &mut *subscription;

    // Check if subscription is initialized
    if subscription_ref.state
        != nano_ros_subscription_state_t::NANO_ROS_SUBSCRIPTION_STATE_INITIALIZED
    {
        return false;
    }

    // Get the callback
    let callback = match subscription_ref.get_callback() {
        Some(cb) => cb,
        None => return false,
    };

    // Get the internal subscriber handle
    let internal = subscription_ref.get_internal();
    if internal.is_null() {
        return false;
    }
    let subscriber = &mut *(internal as *mut ZenohSubscriber);

    // Allocate buffer on stack
    let mut buffer = [0u8; MESSAGE_BUFFER_SIZE];

    // Try to receive a message
    match subscriber.try_recv_raw(&mut buffer) {
        Ok(Some(len)) => {
            // Invoke the user callback with received data
            callback(buffer.as_ptr(), len, subscription_ref.get_context());
            true
        }
        Ok(None) => false,
        Err(_) => false,
    }
}

/// Process a service request if one is available.
///
/// Returns true if a request was processed, false otherwise.
#[cfg(feature = "std")]
unsafe fn process_service_request(service: *mut nano_ros_service_t) -> bool {
    use nano_ros_transport::{ServiceServerTrait, ZenohServiceServer};

    let service_ref = &mut *service;

    // Check if service is initialized
    if service_ref.state != nano_ros_service_state_t::NANO_ROS_SERVICE_STATE_INITIALIZED {
        return false;
    }

    // Get the callback
    let callback = match service_ref.get_callback() {
        Some(cb) => cb,
        None => return false,
    };

    // Get the internal server handle
    let internal = service_ref.get_internal();
    if internal.is_null() {
        return false;
    }
    let server = &mut *(internal as *mut ZenohServiceServer);

    // Allocate buffers on stack
    let mut request_buf = [0u8; MESSAGE_BUFFER_SIZE];
    let mut response_buf = [0u8; MESSAGE_BUFFER_SIZE];

    // Try to receive a request
    let (request_len, sequence_number) = match server.try_recv_request(&mut request_buf) {
        Ok(Some(req)) => (req.data.len(), req.sequence_number),
        Ok(None) => return false,
        Err(_) => return false,
    };

    // Call the user callback
    let mut response_len: usize = 0;
    let handled = callback(
        request_buf.as_ptr(),
        request_len,
        response_buf.as_mut_ptr(),
        MESSAGE_BUFFER_SIZE,
        &mut response_len,
        service_ref.get_context(),
    );

    // Send the response if handled successfully
    if handled && response_len > 0 {
        let _ = server.send_reply(sequence_number, &response_buf[..response_len]);
    }

    true
}

/// Spin the executor once.
///
/// This function checks for ready handles and processes them once.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `timeout_ns` - Timeout in nanoseconds (0 for non-blocking)
///
/// # Returns
/// * `NANO_ROS_RET_OK` if callbacks were executed
/// * `NANO_ROS_RET_TIMEOUT` if no callbacks were ready within timeout
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_spin_some(
    executor: *mut nano_ros_executor_t,
    timeout_ns: u64,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;

    // Accept both INITIALIZED and SPINNING states
    // spin_period/spin set state to SPINNING before calling spin_some
    if executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED
        && executor.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING
    {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Get current time from platform
    let current_time_ns = crate::platform::get_time_ns();

    let mut any_executed = false;

    // Process all handles in order
    for i in 0..executor.handle_count {
        let handle = &mut executor.handles[i];

        match handle.handle_type {
            nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_TIMER => {
                let timer = handle.handle as *mut nano_ros_timer_t;
                if !timer.is_null()
                    && crate::timer::nano_ros_timer_is_ready(timer, current_time_ns) != 0
                {
                    crate::timer::nano_ros_timer_call(timer, current_time_ns);
                    any_executed = true;
                }
            }
            nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_SUBSCRIPTION => {
                #[cfg(feature = "std")]
                {
                    let subscription = handle.handle as *mut nano_ros_subscription_t;
                    if !subscription.is_null() && process_subscription(subscription) {
                        any_executed = true;
                    }
                }
            }
            nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_SERVICE => {
                #[cfg(feature = "std")]
                {
                    let service = handle.handle as *mut nano_ros_service_t;
                    // Try to receive and process a request
                    if !service.is_null() && process_service_request(service) {
                        any_executed = true;
                    }
                }
            }
            nano_ros_executor_handle_type_t::NANO_ROS_EXECUTOR_HANDLE_GUARD_CONDITION => {
                let guard = handle.handle as *mut nano_ros_guard_condition_t;
                if !guard.is_null() {
                    let guard_ref = &mut *guard;
                    // Check if triggered
                    if crate::guard_condition::nano_ros_guard_condition_is_triggered(guard) {
                        // Clear the triggered flag
                        let _ = crate::guard_condition::nano_ros_guard_condition_clear(guard);
                        // Invoke callback if set
                        if let Some(callback) = guard_ref.get_callback() {
                            callback(guard_ref.get_context());
                        }
                        any_executed = true;
                    }
                }
            }
            _ => {}
        }
    }

    // If nothing executed and we have a timeout, wait
    if !any_executed && timeout_ns > 0 {
        // Max 10ms sleep to avoid blocking too long
        crate::platform::sleep_ns(timeout_ns.min(10_000_000));
        return NANO_ROS_RET_TIMEOUT;
    }

    NANO_ROS_RET_OK
}

/// Spin the executor forever.
///
/// This function continuously processes callbacks until shutdown.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
///
/// # Returns
/// * `NANO_ROS_RET_OK` if shutdown gracefully
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_spin(
    executor: *mut nano_ros_executor_t,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    if executor_ref.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    executor_ref.state = nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING;

    // Spin until shutdown
    while executor_ref.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING {
        let _ = nano_ros_executor_spin_some(executor, executor_ref.timeout_ns);
    }

    NANO_ROS_RET_OK
}

/// Spin the executor with a fixed period.
///
/// This function processes callbacks at a fixed rate.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
/// * `period_ns` - Period in nanoseconds
///
/// # Returns
/// * `NANO_ROS_RET_OK` if shutdown gracefully
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL or period is 0
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_spin_period(
    executor: *mut nano_ros_executor_t,
    period_ns: u64,
) -> nano_ros_ret_t {
    if executor.is_null() || period_ns == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    if executor_ref.state != nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    executor_ref.state = nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING;

    // Spin with period using platform time functions
    while executor_ref.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING {
        let start = crate::platform::get_time_ns();

        // Process callbacks
        let _ = nano_ros_executor_spin_some(executor, 0);

        // Sleep for remaining time in period
        let elapsed = crate::platform::get_time_ns().saturating_sub(start);
        if elapsed < period_ns {
            crate::platform::sleep_ns(period_ns - elapsed);
        }
    }

    NANO_ROS_RET_OK
}

/// Stop a spinning executor.
///
/// # Parameters
/// * `executor` - Pointer to a spinning executor
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_stop(
    executor: *mut nano_ros_executor_t,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;

    if executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING {
        executor.state = nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED;
    }

    NANO_ROS_RET_OK
}

/// Finalize an executor.
///
/// # Parameters
/// * `executor` - Pointer to an initialized executor
///
/// # Returns
/// * `NANO_ROS_RET_OK` on success
/// * `NANO_ROS_RET_INVALID_ARGUMENT` if executor is NULL
/// * `NANO_ROS_RET_NOT_INIT` if not initialized
///
/// # Safety
/// * `executor` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_fini(
    executor: *mut nano_ros_executor_t,
) -> nano_ros_ret_t {
    if executor.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;

    if executor.state == nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_UNINITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Clear all handles
    for i in 0..executor.handle_count {
        executor.handles[i] = nano_ros_executor_handle_t::default();
    }

    executor.handle_count = 0;
    executor.support = ptr::null();
    executor.state = nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SHUTDOWN;

    NANO_ROS_RET_OK
}

/// Get the number of handles in the executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_get_handle_count(
    executor: *const nano_ros_executor_t,
) -> c_int {
    if executor.is_null() {
        return 0;
    }

    let executor = &*executor;
    executor.handle_count as c_int
}

/// Check if executor is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_executor_is_valid(executor: *const nano_ros_executor_t) -> c_int {
    if executor.is_null() {
        return 0;
    }

    let executor = &*executor;
    match executor.state {
        nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_INITIALIZED
        | nano_ros_executor_state_t::NANO_ROS_EXECUTOR_STATE_SPINNING => 1,
        _ => 0,
    }
}
