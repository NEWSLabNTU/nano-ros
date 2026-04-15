//! Executor API for nros C API.
//!
//! Thin wrapper over `nros_node::Executor`. All dispatch logic, trigger
//! evaluation, LET semantics, and I/O driving are delegated to the Rust
//! executor — this module only handles C FFI translation.

use core::ffi::c_int;
use core::ptr;

use crate::action::{ActionServerInternal, cancel_callback_trampoline, goal_callback_trampoline};
use crate::action::{
    nros_action_client_state_t, nros_action_client_t, nros_action_server_state_t,
    nros_action_server_t, nros_goal_status_t, nros_goal_uuid_t,
};
use crate::error::*;
use crate::guard_condition::{nros_guard_condition_state_t, nros_guard_condition_t};
use crate::service::{nros_service_state_t, nros_service_t};
use crate::subscription::{nros_subscription_state_t, nros_subscription_t};
use crate::support::{nros_support_state_t, nros_support_t};
use crate::timer::{nros_timer_state_t, nros_timer_t};

pub use crate::config::*;
use crate::constants::NROS_MAX_CONCURRENT_GOALS;

// ============================================================================
// Internal Rust executor type
// ============================================================================

/// The concrete nros-node executor type used by the C API.
///
/// Sizes are configured via `NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE`
/// environment variables at build time (matching nros-node's build.rs).
pub(crate) type CExecutor = nros_node::Executor;

// Compile-time assertion: inline opaque storage must fit the concrete Executor.
#[cfg(any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds"))]
const _: () = assert!(
    core::mem::size_of::<CExecutor>() <= EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
    "EXECUTOR_OPAQUE_U64S too small for Executor — increase NROS_EXECUTOR_ARENA_SIZE \
     or NROS_EXECUTOR_MAX_CBS, or adjust the overhead in build.rs"
);

/// Get a mutable reference to the internal executor from opaque storage.
///
/// # Safety
/// The opaque storage must contain a live, initialized `CExecutor`.
#[inline]
pub(crate) unsafe fn get_executor(opaque: &mut [u64; EXECUTOR_OPAQUE_U64S]) -> &mut CExecutor {
    &mut *(opaque.as_mut_ptr() as *mut CExecutor)
}

/// Get a mutable reference to the internal executor from a raw pointer.
///
/// Used by the action server module which stores a raw pointer to the
/// executor's opaque storage.
///
/// # Safety
/// `ptr` must point to the `_opaque` field of a live, initialized
/// `nros_executor_t`.
#[inline]
pub(crate) unsafe fn get_executor_from_ptr(ptr: *mut core::ffi::c_void) -> &'static mut CExecutor {
    &mut *(ptr as *mut CExecutor)
}

// ============================================================================
// C types (kept for API compatibility)
// ============================================================================

/// Trigger function type for executor.
///
/// A trigger function receives a boolean array indicating which handles have
/// data ready, along with the count of handles. It returns true if the executor
/// should process callbacks.
///
/// # Parameters
/// * `ready` - Pointer to boolean array (one per handle)
/// * `count` - Number of elements in the array
/// * `context` - User-provided context pointer
///
/// # Returns
/// * `true` if executor should process callbacks
/// * `false` if executor should skip processing
pub type nros_executor_trigger_t = Option<
    unsafe extern "C" fn(ready: *const bool, count: usize, context: *mut core::ffi::c_void) -> bool,
>;

/// Callback invocation mode
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_invocation_t {
    /// Only invoke callback when new data is available
    NROS_EXECUTOR_ON_NEW_DATA = 0,
    /// Always invoke callback (even with NULL data)
    NROS_EXECUTOR_ALWAYS = 1,
}

/// Executor data communication semantics
///
/// Defines when data is taken from DDS during spin operations.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_semantics_t {
    /// RCLCPP executor semantics: Data is taken from DDS just before
    /// the corresponding callback is called.
    NROS_SEMANTICS_RCLCPP_EXECUTOR = 0,
    /// Logical Execution Time (LET) semantics: At one sampling point,
    /// new data of all ready subscriptions are taken from DDS.
    /// During sequential processing, the data from that sampling point
    /// is used. New data arriving after the sampling point is not
    /// considered until the next spin iteration.
    NROS_SEMANTICS_LOGICAL_EXECUTION_TIME = 1,
}

/// Executor state
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_executor_state_t {
    /// Not initialized
    NROS_EXECUTOR_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_EXECUTOR_STATE_INITIALIZED = 1,
    /// Currently spinning
    NROS_EXECUTOR_STATE_SPINNING = 2,
    /// Shutdown
    NROS_EXECUTOR_STATE_SHUTDOWN = 3,
}

/// Executor structure.
///
/// The executor delegates all dispatch logic to an internal
/// executor. The C struct retains state, timeout, and
/// per-type counters for API compatibility.
///
/// The internal Rust executor is stored inline in `_opaque` — no heap
/// allocation is needed. The storage size is computed at build time
/// from `NROS_EXECUTOR_MAX_CBS` and `NROS_EXECUTOR_ARENA_SIZE`.
#[repr(C)]
pub struct nros_executor_t {
    /// Current state
    pub state: nros_executor_state_t,
    /// Timeout in nanoseconds for spin_some
    pub timeout_ns: u64,
    /// Data communication semantics
    pub semantics: nros_executor_semantics_t,
    /// Pointer to support context
    pub support: *const nros_support_t,
    /// Trigger function (NULL = default "any" trigger)
    pub trigger: nros_executor_trigger_t,
    /// User context for trigger function
    pub trigger_context: *mut core::ffi::c_void,
    /// Number of handles registered
    pub handle_count: usize,
    /// Maximum handles (configured at init)
    pub max_handles: usize,
    /// Number of subscription handles
    pub subscription_count: usize,
    /// Number of timer handles
    pub timer_count: usize,
    /// Number of service handles
    pub service_count: usize,
    /// Next invocation time in nanoseconds for drift-compensated spin_period
    pub invocation_time_ns: u64,
    /// Inline opaque storage for the Rust executor.
    /// Managed by nros_executor_init/fini — no heap allocation needed.
    pub _opaque: [u64; EXECUTOR_OPAQUE_U64S],
}

impl Default for nros_executor_t {
    fn default() -> Self {
        Self {
            state: nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
            timeout_ns: 100_000_000, // 100ms default
            semantics: nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR,
            support: ptr::null(),
            trigger: None,
            trigger_context: ptr::null_mut(),
            handle_count: 0,
            max_handles: NROS_EXECUTOR_MAX_HANDLES,
            subscription_count: 0,
            timer_count: 0,
            service_count: 0,
            invocation_time_ns: 0,
            #[allow(clippy::large_stack_arrays)] // Intentional: inline opaque storage avoids heap
            _opaque: [0u64; EXECUTOR_OPAQUE_U64S],
        }
    }
}

/// Get a zero-initialized executor.
#[unsafe(no_mangle)]
pub extern "C" fn nros_executor_get_zero_initialized() -> nros_executor_t {
    nros_executor_t::default()
}

/// Initialize an executor.
///
/// # Parameters
/// * `executor` - Pointer to a zero-initialized executor
/// * `support` - Pointer to an initialized support context
/// * `max_handles` - Maximum number of handles (capped at NROS_EXECUTOR_MAX_HANDLES)
///
/// # Returns
/// * `NROS_RET_OK` on success
/// * `NROS_RET_INVALID_ARGUMENT` if any pointer is NULL or max_handles is 0
/// * `NROS_RET_NOT_INIT` if support is not initialized
///
/// # Safety
/// * All pointers must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_init(
    executor: *mut nros_executor_t,
    support: *const nros_support_t,
    max_handles: usize,
) -> nros_ret_t {
    validate_not_null!(executor, support);

    if max_handles == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor = &mut *executor;
    let support_ref = &*support;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
        NROS_RET_BAD_SEQUENCE
    );
    validate_state!(
        support_ref,
        nros_support_state_t::NROS_SUPPORT_STATE_INITIALIZED
    );

    // Create the internal nros-node executor using a borrowed session pointer.
    // Written directly into inline opaque storage — no heap allocation.
    let session_ptr = support_ref.get_session_ptr();
    if session_ptr.is_null() {
        return NROS_RET_NOT_INIT;
    }

    let rust_exec = CExecutor::from_session_ptr(session_ptr);
    ptr::write(executor._opaque.as_mut_ptr() as *mut CExecutor, rust_exec);

    executor.max_handles = max_handles.min(NROS_EXECUTOR_MAX_HANDLES);
    executor.handle_count = 0;
    executor.support = support;
    executor.timeout_ns = 100_000_000; // 100ms default
    executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;

    NROS_RET_OK
}

/// Set the executor timeout.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_timeout(
    executor: *mut nros_executor_t,
    timeout_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.timeout_ns = timeout_ns;
    NROS_RET_OK
}

/// Set data communication semantics.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_semantics(
    executor: *mut nros_executor_t,
    semantics: nros_executor_semantics_t,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.semantics = semantics;

    // Forward to the internal executor
    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        let rust_exec = get_executor(&mut executor._opaque);
        rust_exec.set_semantics(match semantics {
            nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR => {
                nros_node::ExecutorSemantics::RclcppExecutor
            }
            nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME => {
                nros_node::ExecutorSemantics::LogicalExecutionTime
            }
        });
    }

    NROS_RET_OK
}

/// Set the trigger condition for the executor.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_set_trigger(
    executor: *mut nros_executor_t,
    trigger: nros_executor_trigger_t,
    context: *mut core::ffi::c_void,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED
        || executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN
    {
        return NROS_RET_NOT_INIT;
    }

    executor.trigger = trigger;
    executor.trigger_context = context;

    // Forward to the internal executor
    let rust_exec = get_executor(&mut executor._opaque);
    match trigger {
        Some(cb) => {
            rust_exec.set_trigger(nros_node::Trigger::RawPredicate {
                callback: cb,
                context,
            });
        }
        None => {
            rust_exec.set_trigger(nros_node::Trigger::Any);
        }
    }

    NROS_RET_OK
}

// ============================================================================
// Built-in trigger functions (kept as C-exported convenience wrappers)
// ============================================================================

/// Built-in trigger: fire when ANY handle has data ready.
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_any(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = context;
    for i in 0..count {
        if *ready.add(i) {
            return true;
        }
    }
    false
}

/// Built-in trigger: fire when ALL handles have data ready.
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_all(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = context;
    if count == 0 {
        return false;
    }
    for i in 0..count {
        if !*ready.add(i) {
            return false;
        }
    }
    true
}

/// Built-in trigger: always fire (unconditionally).
///
/// # Safety
/// * `ready` and `count` are unused
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_always(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let _ = (ready, count, context);
    true
}

/// Built-in trigger: fire when the handle at the index stored in context has data.
///
/// Pass the handle index (cast to `void*`) as the context parameter.
///
/// # Safety
/// * `ready` must point to a valid array of at least `count` booleans
/// * `context` is interpreted as a `size_t` index
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_trigger_one(
    ready: *const bool,
    count: usize,
    context: *mut core::ffi::c_void,
) -> bool {
    let index = context as usize;
    if index < count {
        *ready.add(index)
    } else {
        false
    }
}

// ============================================================================
// Handle registration — delegated to nros-node Executor
// ============================================================================

/// Add a subscription to the executor.
///
/// Extracts metadata from the subscription struct and registers a raw-bytes
/// callback with the internal nros-node executor. The RMW subscriber handle
/// is created here (moved from subscription init).
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_subscription(
    executor: *mut nros_executor_t,
    subscription: *mut nros_subscription_t,
    invocation: nros_executor_invocation_t,
) -> nros_ret_t {
    validate_not_null!(executor, subscription);

    let executor = &mut *executor;
    let subscription_ref = &*subscription;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        subscription_ref,
        nros_subscription_state_t::NROS_SUBSCRIPTION_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Extract metadata from subscription struct
        let topic_str = core::str::from_utf8_unchecked(
            &subscription_ref.topic_name[..subscription_ref.topic_name_len],
        );
        let type_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_name[..subscription_ref.type_name_len],
        );
        let type_hash_str = core::str::from_utf8_unchecked(
            &subscription_ref.type_hash[..subscription_ref.type_hash_len],
        );

        // Get QoS settings from the subscription
        let qos = subscription_ref.get_qos_settings();

        // Get callback and context
        let callback = match subscription_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let context = subscription_ref.get_context();

        // Register with the nros-node executor using MESSAGE_BUFFER_SIZE
        let result = rust_exec.add_subscription_raw_with_qos_sized::<MESSAGE_BUFFER_SIZE>(
            topic_str,
            type_str,
            type_hash_str,
            qos,
            callback,
            context,
        );

        match result {
            Ok(handle_id) => {
                // Store the handle ID in the subscription for later reference
                let sub_mut = &mut *subscription;
                sub_mut.set_handle_id(handle_id);

                // Set invocation mode
                if invocation == nros_executor_invocation_t::NROS_EXECUTOR_ALWAYS {
                    rust_exec.set_invocation(handle_id, nros_node::InvocationMode::Always);
                }

                executor.handle_count += 1;
                executor.subscription_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a timer to the executor.
///
/// Wraps the C timer callback in a closure and registers it with the
/// internal nros-node executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_timer(
    executor: *mut nros_executor_t,
    timer: *mut nros_timer_t,
) -> nros_ret_t {
    validate_not_null!(executor, timer);

    let executor = &mut *executor;
    let timer_ref = &*timer;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(timer_ref, nros_timer_state_t::NROS_TIMER_STATE_RUNNING);

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Get the C callback and context from the timer
        let c_callback = match timer_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let c_context = timer_ref.get_context();
        let timer_ptr = timer;

        // Wrap the C callback in a Rust closure
        let wrapper = move || {
            // SAFETY: The C callback and timer/context pointers remain valid
            // for the lifetime of the executor (same guarantee as rclc).
            c_callback(timer_ptr, c_context);
        };

        // Convert period from nanoseconds to milliseconds
        let period_ms = timer_ref.period_ns / 1_000_000;
        if period_ms == 0 {
            return NROS_RET_INVALID_ARGUMENT;
        }

        // Register with the nros-node executor
        let period = nros_node::TimerDuration::from_millis(period_ms);
        match rust_exec.add_timer(period, wrapper) {
            Ok(handle_id) => {
                // Store handle ID and executor pointer for cancel/reset operations
                let timer_mut = &mut *timer;
                timer_mut.set_handle_id(handle_id);
                timer_mut.set_executor_ptr(executor._opaque.as_mut_ptr() as *mut core::ffi::c_void);

                executor.handle_count += 1;
                executor.timer_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a service to the executor.
///
/// Extracts metadata from the service struct and registers a raw-bytes
/// service callback with the internal nros-node executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_service(
    executor: *mut nros_executor_t,
    service: *mut nros_service_t,
) -> nros_ret_t {
    validate_not_null!(executor, service);

    let executor = &mut *executor;
    let service_ref = &*service;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        service_ref,
        nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Extract metadata from service struct
        let service_name = core::str::from_utf8_unchecked(
            &service_ref.service_name[..service_ref.service_name_len],
        );
        let type_str =
            core::str::from_utf8_unchecked(&service_ref.type_name[..service_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&service_ref.type_hash[..service_ref.type_hash_len]);

        // Get callback and context
        let callback = match service_ref.get_callback() {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };
        let context = service_ref.get_context();

        // Register with the nros-node executor
        let result = rust_exec.add_service_raw_sized::<MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE>(
            service_name,
            type_str,
            type_hash_str,
            callback,
            context,
        );

        match result {
            Ok(handle_id) => {
                let service_mut = &mut *service;
                service_mut.set_handle_id(handle_id);

                executor.handle_count += 1;
                executor.service_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add a guard condition to the executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_guard_condition(
    executor: *mut nros_executor_t,
    guard: *mut nros_guard_condition_t,
) -> nros_ret_t {
    validate_not_null!(executor, guard);

    let executor = &mut *executor;
    let guard_ref = &*guard;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        guard_ref,
        nros_guard_condition_state_t::NROS_GUARD_CONDITION_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Get the C callback and context from the guard condition
        let c_callback = guard_ref.get_callback();
        let c_context = guard_ref.get_context();

        // Wrap the C callback in a Rust closure
        let wrapper = move || {
            if let Some(cb) = c_callback {
                // SAFETY: The C callback and context remain valid for the
                // lifetime of the executor.
                cb(c_context);
            }
        };

        match rust_exec.add_guard_condition(wrapper) {
            Ok((handle_id, guard_handle)) => {
                let guard_mut = &mut *guard;
                guard_mut.set_handle_id(handle_id);
                guard_mut.set_guard_handle(guard_handle);

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Add an action server to the executor.
///
/// Extracts metadata from the action server struct, creates callback
/// trampolines, and registers with the internal executor.
///
/// # Safety
/// * All pointers must be valid and point to initialized objects
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_action_server(
    executor: *mut nros_executor_t,
    server: *mut nros_action_server_t,
) -> nros_ret_t {
    validate_not_null!(executor, server);

    let executor = &mut *executor;
    let server_ref = &*server;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        server_ref,
        nros_action_server_state_t::NROS_ACTION_SERVER_STATE_INITIALIZED
    );

    // Check capacity
    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        // Grab the opaque pointer before borrowing for get_executor to avoid double borrow.
        let opaque_ptr = executor._opaque.as_mut_ptr() as *mut core::ffi::c_void;
        let rust_exec = get_executor_from_ptr(opaque_ptr);

        // Extract metadata from action server struct
        let action_name =
            core::str::from_utf8_unchecked(&server_ref.action_name[..server_ref.action_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&server_ref.type_name[..server_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&server_ref.type_hash[..server_ref.type_hash_len]);

        // Get the goal callback (required — validated during init)
        let c_goal_callback = match server_ref.goal_callback {
            Some(cb) => cb,
            None => return NROS_RET_INVALID_ARGUMENT,
        };

        // Create the internal struct (handle filled after registration).
        // Written directly into the server's inline `_internal` storage — no heap allocation.
        let internal = ActionServerInternal {
            handle: None,
            executor_ptr: opaque_ptr,
            c_goal_callback,
            c_cancel_callback: server_ref.cancel_callback,
            c_accepted_callback: server_ref.accepted_callback,
            c_context: server_ref.context,
            server_ptr: server,
        };

        let server_mut = &mut *server;
        core::ptr::write(
            server_mut._internal.as_mut_ptr() as *mut ActionServerInternal,
            internal,
        );
        let context = server_mut._internal.as_mut_ptr() as *mut core::ffi::c_void;

        // Register with the nros-node executor using trampolines. The
        // accepted_callback_trampoline is invoked by the arena *after* the
        // accept reply is sent, so the user's long-running execution does
        // not delay the reply the client is blocking on.
        let result = rust_exec
            .add_action_server_raw_sized::<MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_SIZE, NROS_MAX_CONCURRENT_GOALS>(
                action_name,
                type_str,
                type_hash_str,
                goal_callback_trampoline,
                cancel_callback_trampoline,
                Some(crate::action::accepted_callback_trampoline),
                context,
            );

        match result {
            Ok(handle) => {
                // Fill in the handle now that registration succeeded
                let internal_ref =
                    &mut *(server_mut._internal.as_mut_ptr() as *mut ActionServerInternal);
                internal_ref.handle = Some(handle);

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => {
                // Registration failed — drop the internal and zero the storage
                core::ptr::drop_in_place(
                    server_mut._internal.as_mut_ptr() as *mut ActionServerInternal
                );
                server_mut._internal = [0u64; ACTION_SERVER_INTERNAL_OPAQUE_U64S];
                NROS_RET_ERROR
            }
        }
    }
}

/// Register an action client with the executor for async (non-blocking) operation.
///
/// After registration, `nros_executor_spin_some` polls the action client's
/// pending requests (goal response, feedback, result) and invokes the
/// registered callbacks.
///
/// The action client must already be initialized via `nros_action_client_init`.
/// Callbacks should be set via `nros_action_client_set_goal_response_callback`,
/// `nros_action_client_set_feedback_callback`, and `nros_action_client_set_result_callback`
/// before or after this call.
///
/// # Safety
/// * `executor` and `client` must be valid pointers to initialized structs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_add_action_client(
    executor: *mut nros_executor_t,
    client: *mut nros_action_client_t,
) -> nros_ret_t {
    validate_not_null!(executor, client);

    let executor = &mut *executor;
    let client_ref = &*client;

    validate_state!(
        executor,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );
    validate_state!(
        client_ref,
        nros_action_client_state_t::NROS_ACTION_CLIENT_STATE_INITIALIZED
    );

    if executor.handle_count >= executor.max_handles {
        return NROS_RET_FULL;
    }

    {
        let opaque_ptr = executor._opaque.as_mut_ptr() as *mut core::ffi::c_void;
        let rust_exec = get_executor_from_ptr(opaque_ptr);

        // Always register trampolines — they check the C struct's callback
        // pointer at invocation time, so they handle None gracefully. This is
        // critical: the blocking wrappers (nros_action_send_goal, etc.) install
        // temporary callbacks on the C struct AFTER registration. If we only
        // register trampolines when callbacks are non-None at registration time,
        // the arena will consume replies without invoking the trampoline,
        // causing the blocking wrapper's flag to never be set (→ timeout).
        let goal_response_cb: Option<nros_node::executor::RawGoalResponseCallback> =
            Some(goal_response_trampoline as nros_node::executor::RawGoalResponseCallback);

        let feedback_cb: Option<nros_node::executor::RawFeedbackCallback> =
            Some(feedback_trampoline as nros_node::executor::RawFeedbackCallback);

        let result_cb: Option<nros_node::executor::RawResultCallback> =
            Some(result_trampoline as nros_node::executor::RawResultCallback);

        let client_ctx = client as *mut core::ffi::c_void;

        let action_name =
            core::str::from_utf8_unchecked(&client_ref.action_name[..client_ref.action_name_len]);
        let type_str =
            core::str::from_utf8_unchecked(&client_ref.type_name[..client_ref.type_name_len]);
        let type_hash_str =
            core::str::from_utf8_unchecked(&client_ref.type_hash[..client_ref.type_hash_len]);

        // Create a NEW ActionClientCore in the arena via add_action_client_raw.
        // The async send functions will use this core (not the client's original).
        // Both share the same global zenoh session, so the arena core's service
        // clients can communicate with the server independently.
        let result = rust_exec.add_action_client_raw(
            action_name,
            type_str,
            type_hash_str,
            goal_response_cb,
            feedback_cb,
            result_cb,
            client_ctx,
        );

        match result {
            Ok(handle) => {
                let client_mut = &mut *client;
                let int_ref = &mut *(client_mut._internal.as_mut_ptr()
                    as *mut crate::action::ActionClientInternal);
                int_ref.arena_entry_index = handle.entry_index() as i32;
                int_ref.executor_ptr = opaque_ptr;

                executor.handle_count += 1;
                NROS_RET_OK
            }
            Err(_) => NROS_RET_ERROR,
        }
    }
}

/// Goal response trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn goal_response_trampoline(
    goal_id: *const nros_core::GoalId,
    accepted: bool,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.goal_response_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        cb(&uuid, accepted, client.context);
    }
}

/// Feedback trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn feedback_trampoline(
    goal_id: *const nros_core::GoalId,
    feedback_data: *const u8,
    feedback_len: usize,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.feedback_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        cb(&uuid, feedback_data, feedback_len, client.context);
    }
}

/// Result trampoline — adapts nros-node callback to C API callback.
///
/// # Safety
/// `context` must point to a valid `nros_action_client_t`.
unsafe extern "C" fn result_trampoline(
    goal_id: *const nros_core::GoalId,
    status: nros_core::GoalStatus,
    result_data: *const u8,
    result_len: usize,
    context: *mut core::ffi::c_void,
) {
    let client = &*(context as *const nros_action_client_t);
    if let Some(cb) = client.result_callback {
        let uuid = nros_goal_uuid_t {
            uuid: (*goal_id).uuid,
        };
        let c_status = match status {
            nros_core::GoalStatus::Succeeded => nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
            nros_core::GoalStatus::Canceled => nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
            nros_core::GoalStatus::Aborted => nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
            _ => nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
        };
        cb(&uuid, c_status, result_data, result_len, client.context);
    }
}

// ============================================================================
// Spin functions — delegated to nros-node executor
// ============================================================================

/// Spin the executor once.
///
/// Drives middleware I/O, then dispatches ready callbacks.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_some(
    executor: *mut nros_executor_t,
    timeout_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    // Accept both INITIALIZED and SPINNING states
    if executor.state != nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        && executor.state != nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        return NROS_RET_NOT_INIT;
    }

    {
        let rust_exec = get_executor(&mut executor._opaque);

        // Convert timeout from nanoseconds to milliseconds (i32) for nros-node
        let timeout_ms: i32 = if timeout_ns > 0 {
            ((timeout_ns / 1_000_000).max(1)).min(i32::MAX as u64) as i32
        } else {
            0
        };

        // spin_once drives I/O internally and handles trigger evaluation,
        // LET semantics, and dispatch
        let result = rust_exec.spin_once(timeout_ms);

        if result.any_work() {
            NROS_RET_OK
        } else {
            NROS_RET_TIMEOUT
        }
    }
}

/// Spin the executor forever.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor_ref = &mut *executor;

    validate_state!(
        executor_ref,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );

    executor_ref.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING;

    // Spin until shutdown
    while executor_ref.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        let _ = nros_executor_spin_some(executor, executor_ref.timeout_ns);
    }

    NROS_RET_OK
}

/// Spin the executor with a fixed period.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_period(
    executor: *mut nros_executor_t,
    period_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    if period_ns == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    validate_state!(
        executor_ref,
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
    );

    executor_ref.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING;
    executor_ref.invocation_time_ns = crate::platform::get_time_ns();

    while executor_ref.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        // Pass period_ns as the timeout so that spin_once uses it as the
        // timer delta — timers accumulate elapsed time from this value.
        // drive_io() will block for up to period_ms waiting for I/O.
        let _ = nros_executor_spin_some(executor, period_ns);

        // Accumulate next invocation time to prevent drift
        executor_ref.invocation_time_ns += period_ns;
        let now = crate::platform::get_time_ns();
        if executor_ref.invocation_time_ns > now {
            crate::platform::sleep_ns(executor_ref.invocation_time_ns - now);
        }
    }

    NROS_RET_OK
}

/// Spin the executor for one period.
///
/// # Safety
/// * `executor` must be a valid pointer to an initialized executor
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_spin_one_period(
    executor: *mut nros_executor_t,
    period_ns: u64,
) -> nros_ret_t {
    validate_not_null!(executor);

    if period_ns == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let executor_ref = &mut *executor;

    if executor_ref.state != nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        && executor_ref.state != nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING
    {
        return NROS_RET_NOT_INIT;
    }

    let start = crate::platform::get_time_ns();

    // Pass period_ns as the timeout so that spin_once uses it as the
    // timer delta — timers accumulate elapsed time from this value.
    let _ = nros_executor_spin_some(executor, period_ns);

    // Sleep for remaining time in period
    let elapsed = crate::platform::get_time_ns().saturating_sub(start);
    if elapsed < period_ns {
        crate::platform::sleep_ns(period_ns - elapsed);
    }

    NROS_RET_OK
}

/// Stop a spinning executor.
///
/// # Safety
/// * `executor` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_stop(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING {
        executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
    }

    NROS_RET_OK
}

/// Finalize an executor.
///
/// # Safety
/// * `executor` must be a valid pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_fini(executor: *mut nros_executor_t) -> nros_ret_t {
    validate_not_null!(executor);

    let executor = &mut *executor;

    if executor.state == nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    // Drop the internal executor in-place — arena entries are cleaned up
    core::ptr::drop_in_place(executor._opaque.as_mut_ptr() as *mut CExecutor);
    #[allow(clippy::large_stack_arrays)] // Intentional: zero-fill inline opaque storage
    {
        executor._opaque = [0u64; EXECUTOR_OPAQUE_U64S];
    }
    executor.handle_count = 0;
    executor.subscription_count = 0;
    executor.timer_count = 0;
    executor.service_count = 0;
    executor.support = ptr::null();
    executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_SHUTDOWN;

    NROS_RET_OK
}

// ============================================================================
// Query functions
// ============================================================================

/// Get the number of handles in the executor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_get_handle_count(executor: *const nros_executor_t) -> c_int {
    if executor.is_null() {
        return 0;
    }

    let executor = &*executor;
    executor.handle_count as c_int
}

/// Check if executor is valid (initialized).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_is_valid(executor: *const nros_executor_t) -> c_int {
    if executor.is_null() {
        return 0;
    }

    let executor = &*executor;
    match executor.state {
        nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED
        | nros_executor_state_t::NROS_EXECUTOR_STATE_SPINNING => 1,
        _ => 0,
    }
}

/// Get remaining total handle capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_get_remaining_handles(
    executor: *const nros_executor_t,
) -> c_int {
    if executor.is_null() {
        return -1;
    }

    let executor = &*executor;
    (executor.max_handles - executor.handle_count) as c_int
}

// ============================================================================
// Kani verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::error::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn executor_init_null_ptrs() {
        // NULL executor → INVALID_ARGUMENT
        let support = crate::support::nros_support_get_zero_initialized();
        assert_eq!(
            unsafe { nros_executor_init(core::ptr::null_mut(), &support, 4) },
            NROS_RET_INVALID_ARGUMENT,
        );

        // NULL support → INVALID_ARGUMENT
        let mut executor = nros_executor_get_zero_initialized();
        assert_eq!(
            unsafe { nros_executor_init(&mut executor, core::ptr::null(), 4) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn executor_zero_initialized_state() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(
            executor.state,
            nros_executor_state_t::NROS_EXECUTOR_STATE_UNINITIALIZED,
        );
        assert!(executor.support.is_null());
        assert_eq!(executor.handle_count, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::{nros_support_get_zero_initialized, nros_support_state_t};

    #[test]
    fn test_trigger_any_matches_behavior() {
        unsafe {
            let ready = [true, false, true];
            assert!(nros_executor_trigger_any(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [false, false, false];
            assert!(!nros_executor_trigger_any(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            assert!(!nros_executor_trigger_any([].as_ptr(), 0, ptr::null_mut()));
        }
    }

    #[test]
    fn test_trigger_all_matches_behavior() {
        unsafe {
            let ready = [true, true, true];
            assert!(nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [true, false, true];
            assert!(!nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            let ready = [false, false, false];
            assert!(!nros_executor_trigger_all(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));

            assert!(!nros_executor_trigger_all([].as_ptr(), 0, ptr::null_mut()));
        }
    }

    #[test]
    fn test_trigger_always_matches_behavior() {
        unsafe {
            assert!(nros_executor_trigger_always(
                [].as_ptr(),
                0,
                ptr::null_mut()
            ));

            let ready = [false, false];
            assert!(nros_executor_trigger_always(
                ready.as_ptr(),
                ready.len(),
                ptr::null_mut()
            ));
        }
    }

    #[test]
    fn test_trigger_one_matches_behavior() {
        unsafe {
            let ready = [false, true, false];

            assert!(nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                1usize as *mut core::ffi::c_void,
            ));

            assert!(!nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                0usize as *mut core::ffi::c_void,
            ));

            assert!(!nros_executor_trigger_one(
                ready.as_ptr(),
                ready.len(),
                10usize as *mut core::ffi::c_void,
            ));
        }
    }

    #[test]
    fn test_trigger_all_matches_rust_behavior() {
        let test_cases: &[(&[bool], bool)] = &[
            (&[true, true, true], true),
            (&[true, false, true], false),
            (&[false, false, false], false),
            (&[true], true),
            (&[false], false),
            (&[], false),
        ];

        for (case, expected) in test_cases {
            let c_result =
                unsafe { nros_executor_trigger_all(case.as_ptr(), case.len(), ptr::null_mut()) };
            assert_eq!(
                c_result, *expected,
                "trigger_all mismatch for {:?}: got {}, expected {}",
                case, c_result, expected
            );
        }
    }

    #[test]
    fn test_set_trigger_requires_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();

            let ret = nros_executor_set_trigger(
                &mut executor,
                Some(nros_executor_trigger_all),
                ptr::null_mut(),
            );
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_set_trigger_null_executor() {
        unsafe {
            let ret = nros_executor_set_trigger(
                ptr::null_mut(),
                Some(nros_executor_trigger_all),
                ptr::null_mut(),
            );
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_set_semantics_rclcpp() {
        unsafe {
            // Manually initialize (no real session needed for semantics test)
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = 4;

            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR
            );

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR,
            );
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_RCLCPP_EXECUTOR
            );
        }
    }

    #[test]
    fn test_set_semantics_let() {
        unsafe {
            // Manually initialize (no real session needed for semantics test)
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = 4;

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME,
            );
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(
                executor.semantics,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME
            );
        }
    }

    #[test]
    fn test_set_semantics_requires_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();

            let ret = nros_executor_set_semantics(
                &mut executor,
                nros_executor_semantics_t::NROS_SEMANTICS_LOGICAL_EXECUTION_TIME,
            );
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_spin_one_period_null() {
        unsafe {
            let ret = nros_executor_spin_one_period(ptr::null_mut(), 10_000_000);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_one_period_zero_period() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_one_period(&mut executor, 0);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_one_period_not_init() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_one_period(&mut executor, 10_000_000);
            assert_eq!(ret, NROS_RET_NOT_INIT);
        }
    }

    #[test]
    fn test_spin_period_null() {
        unsafe {
            let ret = nros_executor_spin_period(ptr::null_mut(), 10_000_000);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_spin_period_zero_period() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            let ret = nros_executor_spin_period(&mut executor, 0);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_invocation_time_ns_initialized() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(executor.invocation_time_ns, 0);
    }

    #[test]
    fn test_per_type_counters_initialized_to_zero() {
        let executor = nros_executor_get_zero_initialized();
        assert_eq!(executor.subscription_count, 0);
        assert_eq!(executor.timer_count, 0);
        assert_eq!(executor.service_count, 0);
    }

    #[test]
    fn test_remaining_handles_null() {
        unsafe {
            assert_eq!(nros_executor_get_remaining_handles(ptr::null()), -1);
        }
    }

    #[test]
    fn test_remaining_capacity_initial() {
        unsafe {
            let mut executor = nros_executor_get_zero_initialized();
            executor.state = nros_executor_state_t::NROS_EXECUTOR_STATE_INITIALIZED;
            executor.max_handles = NROS_EXECUTOR_MAX_HANDLES;

            assert_eq!(
                nros_executor_get_remaining_handles(&executor),
                NROS_EXECUTOR_MAX_HANDLES as c_int
            );
        }
    }

    #[test]
    fn test_max_handles_equals_max_cbs() {
        assert_eq!(NROS_EXECUTOR_MAX_HANDLES, nros_node::config::MAX_CBS);
    }
}
