//! Validation macros for C API entry points.
//!
//! These macros reduce boilerplate in FFI functions that need to validate
//! null pointers and object state before proceeding.

/// Validate that none of the given pointers are null.
///
/// Returns `NROS_RET_INVALID_ARGUMENT` if any pointer is null.
///
/// # Example
///
/// ```ignore
/// validate_not_null!(publisher, node, type_info, topic_name);
/// ```
macro_rules! validate_not_null {
    ($($ptr:expr),+ $(,)?) => {
        if $($ptr.is_null())||+ {
            return NROS_RET_INVALID_ARGUMENT;
        }
    };
}

/// Validate that an object's `state` field equals the expected value.
///
/// Two-argument form returns `NROS_RET_NOT_INIT`:
/// ```ignore
/// validate_state!(service, nros_service_state_t::NROS_SERVICE_STATE_INITIALIZED);
/// ```
///
/// Three-argument form returns the specified error:
/// ```ignore
/// validate_state!(service, nros_service_state_t::NROS_SERVICE_STATE_UNINITIALIZED, NROS_RET_BAD_SEQUENCE);
/// ```
macro_rules! validate_state {
    ($obj:expr, $expected:expr) => {
        if $obj.state != $expected {
            return NROS_RET_NOT_INIT;
        }
    };
    ($obj:expr, $expected:expr, $err:expr) => {
        if $obj.state != $expected {
            return $err;
        }
    };
}
