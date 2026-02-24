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
pub enum nros_goal_status_t {
    /// Goal state is unknown
    NROS_GOAL_STATUS_UNKNOWN = 0,
    /// Goal was accepted and is pending execution
    NROS_GOAL_STATUS_ACCEPTED = 1,
    /// Goal is currently being executed
    NROS_GOAL_STATUS_EXECUTING = 2,
    /// Goal is being canceled
    NROS_GOAL_STATUS_CANCELING = 3,
    /// Goal completed successfully
    NROS_GOAL_STATUS_SUCCEEDED = 4,
    /// Goal was canceled
    NROS_GOAL_STATUS_CANCELED = 5,
    /// Goal was aborted (failed)
    NROS_GOAL_STATUS_ABORTED = 6,
}

/// Goal response codes for goal request handling.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_goal_response_t {
    /// Reject the goal
    NROS_GOAL_REJECT = 0,
    /// Accept the goal and start executing immediately
    NROS_GOAL_ACCEPT_AND_EXECUTE = 1,
    /// Accept the goal but defer execution
    NROS_GOAL_ACCEPT_AND_DEFER = 2,
}

/// Cancel response codes.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_cancel_response_t {
    /// Reject the cancel request
    NROS_CANCEL_REJECT = 0,
    /// Accept the cancel request
    NROS_CANCEL_ACCEPT = 1,
}

// ============================================================================
// Action Type Info
// ============================================================================

/// Action type information.
#[repr(C)]
pub struct nros_action_type_t {
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
}

// ============================================================================
// Goal UUID
// ============================================================================

/// Goal UUID structure (16 bytes).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct nros_goal_uuid_t {
    /// UUID bytes
    pub uuid: [u8; 16],
}

// ============================================================================
// Goal Handle
// ============================================================================

/// Goal handle structure.
#[repr(C)]
pub struct nros_goal_handle_t {
    /// Goal UUID
    pub uuid: nros_goal_uuid_t,
    /// Current status
    pub status: nros_goal_status_t,
    /// Whether this goal slot is in use
    pub active: bool,
    /// User context pointer for this goal
    pub context: *mut c_void,
    /// Pointer back to the action server (internal)
    pub server: *mut super::nros_action_server_t,
}

impl Default for nros_goal_handle_t {
    fn default() -> Self {
        Self {
            uuid: nros_goal_uuid_t::default(),
            status: nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            active: false,
            context: core::ptr::null_mut(),
            server: core::ptr::null_mut(),
        }
    }
}

// ============================================================================
// Callback Types
// ============================================================================

/// Goal request callback type.
pub type nros_goal_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        goal_request: *const u8,
        goal_len: usize,
        context: *mut c_void,
    ) -> nros_goal_response_t,
>;

/// Cancel request callback type.
pub type nros_cancel_callback_t = Option<
    unsafe extern "C" fn(
        goal: *mut nros_goal_handle_t,
        context: *mut c_void,
    ) -> nros_cancel_response_t,
>;

/// Goal accepted callback type.
pub type nros_accepted_callback_t =
    Option<unsafe extern "C" fn(goal: *mut nros_goal_handle_t, context: *mut c_void)>;

/// Feedback callback type (for client).
pub type nros_feedback_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        feedback: *const u8,
        feedback_len: usize,
        context: *mut c_void,
    ),
>;

/// Result callback type (for client).
pub type nros_result_callback_t = Option<
    unsafe extern "C" fn(
        goal_uuid: *const nros_goal_uuid_t,
        status: nros_goal_status_t,
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
pub unsafe extern "C" fn nros_goal_uuid_generate(uuid: *mut nros_goal_uuid_t) -> nros_ret_t {
    if uuid.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let uuid = &mut *uuid;

    #[cfg(feature = "std")]
    {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Simple UUID generation using system time and a counter
        // Not cryptographically secure, but sufficient for goal IDs
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let nanos = now.as_nanos() as u64;
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Fill UUID with time-based values
        uuid.uuid[0..8].copy_from_slice(&nanos.to_le_bytes());
        uuid.uuid[8..16].copy_from_slice(&count.to_le_bytes());

        // Set version (4) and variant bits for UUID v4-like format
        uuid.uuid[6] = (uuid.uuid[6] & 0x0f) | 0x40;
        uuid.uuid[8] = (uuid.uuid[8] & 0x3f) | 0x80;

        NROS_RET_OK
    }

    #[cfg(not(feature = "std"))]
    {
        // For no_std, use a simple counter-based approach
        static mut COUNTER: u64 = 0;
        COUNTER = COUNTER.wrapping_add(1);

        uuid.uuid = [0u8; 16];
        let counter_bytes = COUNTER.to_le_bytes();
        uuid.uuid[0..8].copy_from_slice(&counter_bytes);

        NROS_RET_OK
    }
}

/// Compare two goal UUIDs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_goal_uuid_equal(
    a: *const nros_goal_uuid_t,
    b: *const nros_goal_uuid_t,
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
pub extern "C" fn nros_goal_status_to_string(status: nros_goal_status_t) -> *const c_char {
    match status {
        nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN => c"UNKNOWN".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED => c"ACCEPTED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING => c"EXECUTING".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_CANCELING => c"CANCELING".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED => c"SUCCEEDED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_CANCELED => c"CANCELED".as_ptr(),
        nros_goal_status_t::NROS_GOAL_STATUS_ABORTED => c"ABORTED".as_ptr(),
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
            unsafe { nros_goal_uuid_generate(ptr::null_mut()) },
            NROS_RET_INVALID_ARGUMENT,
        );
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_uuid_equal_null() {
        let uuid = nros_goal_uuid_t::default();

        assert!(!unsafe { nros_goal_uuid_equal(ptr::null(), &uuid) });
        assert!(!unsafe { nros_goal_uuid_equal(&uuid, ptr::null()) });
        assert!(!unsafe { nros_goal_uuid_equal(ptr::null(), ptr::null()) });
    }

    #[kani::proof]
    #[kani::unwind(5)]
    fn goal_status_to_string_all_variants() {
        let statuses = [
            nros_goal_status_t::NROS_GOAL_STATUS_UNKNOWN,
            nros_goal_status_t::NROS_GOAL_STATUS_ACCEPTED,
            nros_goal_status_t::NROS_GOAL_STATUS_EXECUTING,
            nros_goal_status_t::NROS_GOAL_STATUS_CANCELING,
            nros_goal_status_t::NROS_GOAL_STATUS_SUCCEEDED,
            nros_goal_status_t::NROS_GOAL_STATUS_CANCELED,
            nros_goal_status_t::NROS_GOAL_STATUS_ABORTED,
        ];

        for status in statuses {
            let s = nros_goal_status_to_string(status);
            assert!(!s.is_null());
        }
    }
}
