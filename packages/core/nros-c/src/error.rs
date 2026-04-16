//! Error types and return codes for the C API.

use core::ffi::c_int;

/// Return type for nros C API functions.
///
/// Compatible with rcl_ret_t for familiarity.
pub type nros_ret_t = c_int;

/// Success
pub const NROS_RET_OK: nros_ret_t = 0;

/// Generic error
pub const NROS_RET_ERROR: nros_ret_t = -1;

/// Timeout occurred
pub const NROS_RET_TIMEOUT: nros_ret_t = -2;

/// Invalid argument passed
pub const NROS_RET_INVALID_ARGUMENT: nros_ret_t = -3;

/// Resource not found
pub const NROS_RET_NOT_FOUND: nros_ret_t = -4;

/// Resource already exists
pub const NROS_RET_ALREADY_EXISTS: nros_ret_t = -5;

/// Resource limit reached (e.g., max handles)
pub const NROS_RET_FULL: nros_ret_t = -6;

/// Not initialized
pub const NROS_RET_NOT_INIT: nros_ret_t = -7;

/// Bad sequence (e.g., wrong order of operations)
pub const NROS_RET_BAD_SEQUENCE: nros_ret_t = -8;

/// Service call failed
pub const NROS_RET_SERVICE_FAILED: nros_ret_t = -9;

/// Publish failed
pub const NROS_RET_PUBLISH_FAILED: nros_ret_t = -10;

/// Subscription failed
pub const NROS_RET_SUBSCRIPTION_FAILED: nros_ret_t = -11;

/// Operation not allowed (e.g., goal not in correct state)
pub const NROS_RET_NOT_ALLOWED: nros_ret_t = -12;

/// Request was rejected (e.g., goal rejected by server)
pub const NROS_RET_REJECTED: nros_ret_t = -13;

/// Operation not yet ready (e.g., async response still pending).
/// Caller should spin the executor and try again.
pub const NROS_RET_TRY_AGAIN: nros_ret_t = -14;

/// Reentrant call detected — a blocking helper (`nros_client_call`,
/// `nros_action_send_goal`, `nros_action_get_result`) was called from
/// inside a dispatch callback. These functions internally call
/// `nros_executor_spin_some`, which is not reentrant.
pub const NROS_RET_REENTRANT: nros_ret_t = -15;
