//! Shared types, enums, and utility functions for the action API.

use core::ffi::{c_char, c_void};

use crate::error::*;

// ============================================================================
// Goal Status
// ============================================================================

/// Goal status enumeration.
///
/// Compatible with action_msgs/msg/GoalStatus values.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_goal_status_t {
    /// Goal state is unknown
    RCLC_GOAL_STATUS_UNKNOWN = 0,
    /// Goal was accepted and is pending execution
    RCLC_GOAL_STATUS_ACCEPTED = 1,
    /// Goal is currently being executed
    RCLC_GOAL_STATUS_EXECUTING = 2,
    /// Goal is being canceled
    RCLC_GOAL_STATUS_CANCELING = 3,
    /// Goal completed successfully
    RCLC_GOAL_STATUS_SUCCEEDED = 4,
    /// Goal was canceled
    RCLC_GOAL_STATUS_CANCELED = 5,
    /// Goal was aborted (failed)
    RCLC_GOAL_STATUS_ABORTED = 6,
}

/// Goal response codes for goal request handling.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_goal_response_t {
    /// Reject the goal
    RCLC_GOAL_REJECT = 0,
    /// Accept the goal and start executing immediately
    RCLC_GOAL_ACCEPT_AND_EXECUTE = 1,
    /// Accept the goal but defer execution
    RCLC_GOAL_ACCEPT_AND_DEFER = 2,
}

/// Cancel response codes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum rclc_cancel_response_t {
    /// Reject the cancel request
    RCLC_CANCEL_REJECT = 0,
    /// Accept the cancel request
    RCLC_CANCEL_ACCEPT = 1,
}

// ============================================================================
// Action Type Info
// ============================================================================

/// Action type information.
#[repr(C)]
pub struct rclc_action_type_t {
    /// Action type name (e.g., "example_interfaces::action::Fibonacci")
    pub type_name: *const c_char,
    /// Action type hash
    pub type_hash: *const c_char,
    /// Maximum serialized size of goal message
    pub goal_serialized_size_max: usize,
    /// Maximum serialized size of result message
    pub result_serialized_size_max: usize,
    /// Maximum serialized size of feedback message
    pub feedback_serialized_size_max: usize,
    /// phase-492 W8 — the goal part's serialiser (NULL in a descriptor built
    /// from names alone). Generated as `<pkg>__action__<Name>__nros_type`,
    /// what `ROSIDL_GET_ACTION_TYPE_SUPPORT(pkg, action, Name)` yields.
    pub goal_serialize: crate::publisher::rclc_message_serialize_fn_t,
    /// The goal part's deserialiser.
    pub goal_deserialize: crate::publisher::rclc_message_deserialize_fn_t,
    /// The result part's serialiser.
    pub result_serialize: crate::publisher::rclc_message_serialize_fn_t,
    /// The result part's deserialiser.
    pub result_deserialize: crate::publisher::rclc_message_deserialize_fn_t,
    /// The feedback part's serialiser.
    pub feedback_serialize: crate::publisher::rclc_message_serialize_fn_t,
    /// The feedback part's deserialiser.
    pub feedback_deserialize: crate::publisher::rclc_message_deserialize_fn_t,
}

// ============================================================================
// Goal UUID
// ============================================================================

/// Goal UUID structure (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct rclc_goal_uuid_t {
    /// UUID bytes
    pub uuid: [u8; 16],
}

// ============================================================================
// Goal Handle
// ============================================================================

/// Goal handle — a pure UUID identity card.
///
/// Carries just the goal UUID. All lifecycle state (accepted, executing,
/// cancelling, etc.) and per-goal context are managed outside the handle:
/// status comes from the arena via
/// [`rclc_action_get_goal_status`](super::rclc_action_get_goal_status);
/// per-goal user context is tracked in caller-managed `{uuid → state}`
/// storage. The handle is copyable by value — trampolines build a
/// stack-local and pass it to user callbacks, and users can copy it into
/// their own data structures to reference the goal later.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct rclc_goal_handle_t {
    /// Goal UUID
    pub uuid: rclc_goal_uuid_t,
}

// ============================================================================
// Callback Types
// ============================================================================

/// Goal request callback type.
///
/// Invoked when a client sends a new goal. Return `RCLC_GOAL_REJECT`,
/// `RCLC_GOAL_ACCEPT_AND_EXECUTE`, or `RCLC_GOAL_ACCEPT_AND_DEFER`.
pub type rclc_goal_callback_t = Option<
    unsafe extern "C" fn(
        server: *mut super::rcl_action_server_t,
        goal: *const rclc_goal_handle_t,
        goal_request: *const u8,
        goal_len: usize,
        context: *mut c_void,
    ) -> rclc_goal_response_t,
>;

/// Cancel request callback type.
///
/// Invoked when a client requests cancellation of an accepted goal.
/// Return `RCLC_CANCEL_ACCEPT` or `RCLC_CANCEL_REJECT`.
pub type rclc_cancel_callback_t = Option<
    unsafe extern "C" fn(
        server: *mut super::rcl_action_server_t,
        goal: *const rclc_goal_handle_t,
        context: *mut c_void,
    ) -> rclc_cancel_response_t,
>;

/// Goal accepted callback type.
///
/// Invoked after the arena has sent the accept reply to the client.
/// Use this to kick off goal execution.
pub type rclc_accepted_callback_t = Option<
    unsafe extern "C" fn(
        server: *mut super::rcl_action_server_t,
        goal: *const rclc_goal_handle_t,
        context: *mut c_void,
    ),
>;

/// Goal response callback type (for async client).
/// Called when the action server accepts or rejects a goal.
pub type rclc_goal_response_callback_t = Option<
    unsafe extern "C" fn(goal_uuid: *const rclc_goal_uuid_t, accepted: bool, context: *mut c_void),
>;

/// Feedback callback type (for client).
pub type rclc_feedback_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const rclc_goal_uuid_t,
        feedback: *const u8,
        feedback_len: usize,
        context: *mut c_void,
    ),
>;

/// Result callback type (for client).
pub type rclc_result_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const rclc_goal_uuid_t,
        status: rclc_goal_status_t,
        result: *const u8,
        result_len: usize,
        context: *mut c_void,
    ),
>;

// ============================================================================
// Utility Functions
// ============================================================================

/// Generate a new random goal UUID.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_goal_uuid_generate(uuid: *mut rclc_goal_uuid_t) -> rcl_ret_t {
    if uuid.is_null() {
        return RCL_RET_INVALID_ARGUMENT;
    }

    let uuid = &mut *uuid;

    // phase-359 W10 — one generator, and the no_std one was the bug.
    //
    // The `std` arm mixed a wall-clock nanosecond stamp with a counter. The
    // `no_std` arm used a bare `static mut COUNTER`, so every boot re-issued the
    // same goal IDs from 1 — colliding with the previous run of the same node,
    // and with any sibling node on the bus. The stamp is what carried uniqueness
    // across processes, and target builds simply did not have it.
    //
    // They do: `platform::get_system_time_ns()` reads the ABI wall clock every
    // port implements. This also retires a `static mut`, which edition 2024
    // rejects for exactly the reason it was risky here.
    {
        // AtomicU32, not AtomicU64: 32-bit targets without 64-bit atomics
        // (riscv32imac NuttX, thumbv7m) don't have the type at all (#134); the
        // nanosecond timestamp carries the uniqueness across processes.
        //
        // issue 0700 — `portable_atomic`, not `core::sync::atomic`. On
        // riscv32imc (esp32c3 under ESP-IDF) there are no hardware atomics, so
        // core's `AtomicU32` exists as a TYPE while its read-modify-write
        // methods are gated behind `target_has_atomic` — `fetch_add` is simply
        // absent and the crate failed to compile with E0599. `portable_atomic`
        // polyfills it through the `portable_atomic_unsafe_assume_single_core`
        // cfg (set for that target in the root `.cargo/config.toml`) or a
        // `critical-section` impl. It is an unconditional dependency of this
        // crate, added for exactly this reason, and `log.rs` already uses it —
        // this site was the one that kept naming `core` instead.
        static COUNTER: portable_atomic::AtomicU32 = portable_atomic::AtomicU32::new(0);

        let nanos = crate::platform::get_system_time_ns() as u64;
        let count = COUNTER.fetch_add(1, portable_atomic::Ordering::Relaxed) as u64;

        // Fill UUID with time-based values
        uuid.uuid[0..8].copy_from_slice(&nanos.to_le_bytes());
        uuid.uuid[8..16].copy_from_slice(&count.to_le_bytes());

        // Set version (4) and variant bits for UUID v4-like format
        uuid.uuid[6] = (uuid.uuid[6] & 0x0f) | 0x40;
        uuid.uuid[8] = (uuid.uuid[8] & 0x3f) | 0x80;

        RCL_RET_OK
    }
}

/// Compare two goal UUIDs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rclc_goal_uuid_equal(
    a: *const rclc_goal_uuid_t,
    b: *const rclc_goal_uuid_t,
) -> bool {
    if a.is_null() || b.is_null() {
        return false;
    }

    let a = &*a;
    let b = &*b;

    a.uuid == b.uuid
}

/// Get status name as string.
#[unsafe(no_mangle)]
pub extern "C" fn rclc_goal_status_to_string(status: rclc_goal_status_t) -> *const c_char {
    match status {
        rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN => c"UNKNOWN".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_ACCEPTED => c"ACCEPTED".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_EXECUTING => c"EXECUTING".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELING => c"CANCELING".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED => c"SUCCEEDED".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED => c"CANCELED".as_ptr(),
        rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED => c"ABORTED".as_ptr(),
    }
}

// ============================================================================
// Kani Verification
// ============================================================================

#[cfg(kani)]
mod verification {
    use super::*;
    use core::ptr;

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_uuid_generate_null() {
        assert_eq!(
            unsafe { rclc_goal_uuid_generate(ptr::null_mut()) },
            RCL_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_uuid_equal_null() {
        let uuid = rclc_goal_uuid_t::default();

        assert!(!unsafe { rclc_goal_uuid_equal(ptr::null(), &uuid) });
        assert!(!unsafe { rclc_goal_uuid_equal(&uuid, ptr::null()) });
        assert!(!unsafe { rclc_goal_uuid_equal(ptr::null(), ptr::null()) });
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_status_to_string_all_variants() {
        let statuses = [
            rclc_goal_status_t::RCLC_GOAL_STATUS_UNKNOWN,
            rclc_goal_status_t::RCLC_GOAL_STATUS_ACCEPTED,
            rclc_goal_status_t::RCLC_GOAL_STATUS_EXECUTING,
            rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELING,
            rclc_goal_status_t::RCLC_GOAL_STATUS_SUCCEEDED,
            rclc_goal_status_t::RCLC_GOAL_STATUS_CANCELED,
            rclc_goal_status_t::RCLC_GOAL_STATUS_ABORTED,
        ];

        for status in statuses {
            let s = rclc_goal_status_to_string(status);
            assert!(!s.is_null());
        }
    }
}
