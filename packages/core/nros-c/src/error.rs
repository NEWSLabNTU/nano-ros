//! Error types and return codes for the C API.

use core::ffi::c_int;

/// Return type for nros C API functions.
///
/// Compatible with rcl_ret_t for familiarity.
pub type nano_ros_ret_t = c_int;

/// Success
pub const NANO_ROS_RET_OK: nano_ros_ret_t = 0;

/// Generic error
pub const NANO_ROS_RET_ERROR: nano_ros_ret_t = -1;

/// Timeout occurred
pub const NANO_ROS_RET_TIMEOUT: nano_ros_ret_t = -2;

/// Invalid argument passed
pub const NANO_ROS_RET_INVALID_ARGUMENT: nano_ros_ret_t = -3;

/// Resource not found
pub const NANO_ROS_RET_NOT_FOUND: nano_ros_ret_t = -4;

/// Resource already exists
pub const NANO_ROS_RET_ALREADY_EXISTS: nano_ros_ret_t = -5;

/// Resource limit reached (e.g., max handles)
pub const NANO_ROS_RET_FULL: nano_ros_ret_t = -6;

/// Not initialized
pub const NANO_ROS_RET_NOT_INIT: nano_ros_ret_t = -7;

/// Bad sequence (e.g., wrong order of operations)
pub const NANO_ROS_RET_BAD_SEQUENCE: nano_ros_ret_t = -8;

/// Service call failed
pub const NANO_ROS_RET_SERVICE_FAILED: nano_ros_ret_t = -9;

/// Publish failed
pub const NANO_ROS_RET_PUBLISH_FAILED: nano_ros_ret_t = -10;

/// Subscription failed
pub const NANO_ROS_RET_SUBSCRIPTION_FAILED: nano_ros_ret_t = -11;

/// Operation not allowed (e.g., goal not in correct state)
pub const NANO_ROS_RET_NOT_ALLOWED: nano_ros_ret_t = -12;

/// Request was rejected (e.g., goal rejected by server)
pub const NANO_ROS_RET_REJECTED: nano_ros_ret_t = -13;
