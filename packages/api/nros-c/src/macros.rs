//! Validation macros for C API entry points.
//!
//! These macros reduce boilerplate in FFI functions that need to validate
//! null pointers and object state before proceeding.

/// Validate that none of the given pointers are null.
///
/// Returns `RCL_RET_INVALID_ARGUMENT` if any pointer is null, naming the first
/// null one in the error state `rcl_get_error_string` reads (phase-515 C5).
///
/// # Example
///
/// ```ignore
/// validate_not_null!(publisher, node, type_info, topic_name);
/// ```
#[allow(unused_macros)] // Used in RMW-gated modules; appears unused in no-feature builds
macro_rules! validate_not_null {
    ($($ptr:expr),+ $(,)?) => {
        $(
            if $ptr.is_null() {
                // phase-515 C5 — the message `rcl_get_error_string` reads back.
                $crate::error_handling::set_static(
                    concat!(stringify!($ptr), " argument is null"),
                    file!(),
                    line!(),
                );
                return RCL_RET_INVALID_ARGUMENT;
            }
        )+
    };
}

/// Validate that an object's `state` field equals the expected value.
///
/// Two-argument form returns `RCL_RET_NOT_INIT`:
/// ```ignore
/// validate_state!(service, rclc_service_state_t::RCLC_SERVICE_STATE_INITIALIZED);
/// ```
///
/// Three-argument form returns the specified error:
/// ```ignore
/// validate_state!(service, rclc_service_state_t::RCLC_SERVICE_STATE_UNINITIALIZED, RCLC_RET_BAD_SEQUENCE);
/// ```
#[allow(unused_macros)] // Used in RMW-gated modules; appears unused in no-feature builds
macro_rules! validate_state {
    ($obj:expr, $expected:expr) => {
        if $obj.state != $expected {
            $crate::error_handling::set_static(
                concat!(stringify!($obj), " is not initialized"),
                file!(),
                line!(),
            );
            return RCL_RET_NOT_INIT;
        }
    };
    ($obj:expr, $expected:expr, $err:expr) => {
        if $obj.state != $expected {
            $crate::error_handling::set_static(
                concat!(stringify!($obj), " is not in the state this call requires"),
                file!(),
                line!(),
            );
            return $err;
        }
    };
}
