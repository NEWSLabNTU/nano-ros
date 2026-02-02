//! Parameter API for nano-ros C API.
//!
//! Provides node parameters for configuration and runtime tuning.

use core::ffi::{c_char, c_void};
use core::ptr;

use crate::error::*;

// ============================================================================
// Constants
// ============================================================================

/// Maximum length of a parameter name
pub const NANO_ROS_MAX_PARAM_NAME_LEN: usize = 64;

/// Maximum length of a string parameter value
pub const NANO_ROS_MAX_PARAM_STRING_LEN: usize = 128;

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameter type enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_parameter_type_t {
    /// Parameter not set
    NANO_ROS_PARAMETER_NOT_SET = 0,
    /// Boolean parameter
    NANO_ROS_PARAMETER_BOOL = 1,
    /// 64-bit signed integer parameter
    NANO_ROS_PARAMETER_INTEGER = 2,
    /// 64-bit floating point parameter
    NANO_ROS_PARAMETER_DOUBLE = 3,
    /// String parameter
    NANO_ROS_PARAMETER_STRING = 4,
    /// Byte array parameter (not yet supported)
    NANO_ROS_PARAMETER_BYTE_ARRAY = 5,
    /// Boolean array parameter (not yet supported)
    NANO_ROS_PARAMETER_BOOL_ARRAY = 6,
    /// Integer array parameter (not yet supported)
    NANO_ROS_PARAMETER_INTEGER_ARRAY = 7,
    /// Double array parameter (not yet supported)
    NANO_ROS_PARAMETER_DOUBLE_ARRAY = 8,
    /// String array parameter (not yet supported)
    NANO_ROS_PARAMETER_STRING_ARRAY = 9,
}

/// Parameter value union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union nano_ros_parameter_value_t {
    /// Boolean value
    pub bool_value: bool,
    /// Integer value (64-bit)
    pub integer_value: i64,
    /// Double value
    pub double_value: f64,
    /// String value (fixed-size buffer)
    pub string_value: [u8; NANO_ROS_MAX_PARAM_STRING_LEN],
}

impl Default for nano_ros_parameter_value_t {
    fn default() -> Self {
        Self { integer_value: 0 }
    }
}

/// Parameter structure.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nano_ros_parameter_t {
    /// Parameter name (null-terminated)
    pub name: [u8; NANO_ROS_MAX_PARAM_NAME_LEN],
    /// Parameter type
    pub r#type: nano_ros_parameter_type_t,
    /// Parameter value
    pub value: nano_ros_parameter_value_t,
}

impl Default for nano_ros_parameter_t {
    fn default() -> Self {
        Self {
            name: [0u8; NANO_ROS_MAX_PARAM_NAME_LEN],
            r#type: nano_ros_parameter_type_t::NANO_ROS_PARAMETER_NOT_SET,
            value: nano_ros_parameter_value_t::default(),
        }
    }
}

// ============================================================================
// Parameter Server
// ============================================================================

/// Parameter server state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nano_ros_param_server_state_t {
    /// Not initialized
    NANO_ROS_PARAM_SERVER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NANO_ROS_PARAM_SERVER_STATE_READY = 1,
    /// Shutdown
    NANO_ROS_PARAM_SERVER_STATE_SHUTDOWN = 2,
}

/// Parameter change callback type.
pub type nano_ros_param_callback_t = Option<
    unsafe extern "C" fn(
        name: *const c_char,
        param: *const nano_ros_parameter_t,
        context: *mut c_void,
    ) -> bool,
>;

/// Parameter server structure.
#[repr(C)]
pub struct nano_ros_param_server_t {
    /// Current state
    pub state: nano_ros_param_server_state_t,
    /// Maximum number of parameters
    pub capacity: usize,
    /// Current number of parameters
    pub count: usize,
    /// Parameter storage (pointer to user-provided array)
    parameters: *mut nano_ros_parameter_t,
    /// Parameter change callback
    callback: nano_ros_param_callback_t,
    /// Callback context
    callback_context: *mut c_void,
}

impl Default for nano_ros_param_server_t {
    fn default() -> Self {
        Self {
            state: nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_UNINITIALIZED,
            capacity: 0,
            count: 0,
            parameters: ptr::null_mut(),
            callback: None,
            callback_context: ptr::null_mut(),
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Copy a C string to a fixed-size buffer.
/// Returns the number of bytes copied (excluding null terminator).
unsafe fn copy_cstr_to_buffer(src: *const c_char, dst: &mut [u8]) -> usize {
    if src.is_null() || dst.is_empty() {
        return 0;
    }

    let src = src as *const u8;
    let max_len = dst.len() - 1; // Reserve space for null terminator
    let mut len = 0usize;

    while len < max_len {
        let c = *src.add(len);
        if c == 0 {
            break;
        }
        dst[len] = c;
        len += 1;
    }
    dst[len] = 0; // Null terminate

    len
}

/// Compare a C string with a buffer.
unsafe fn cstr_eq_buffer(cstr: *const c_char, buffer: &[u8]) -> bool {
    if cstr.is_null() {
        return false;
    }

    let cstr = cstr as *const u8;
    let mut i = 0usize;

    loop {
        let c1 = *cstr.add(i);
        let c2 = if i < buffer.len() { buffer[i] } else { 0 };

        if c1 == 0 && c2 == 0 {
            return true;
        }
        if c1 != c2 {
            return false;
        }
        i += 1;
    }
}

// ============================================================================
// Parameter Server Functions
// ============================================================================

/// Get a zero-initialized parameter server.
#[unsafe(no_mangle)]
pub extern "C" fn nano_ros_param_server_get_zero_initialized() -> nano_ros_param_server_t {
    nano_ros_param_server_t::default()
}

/// Initialize a parameter server with user-provided storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_server_init(
    server: *mut nano_ros_param_server_t,
    storage: *mut nano_ros_parameter_t,
    capacity: usize,
) -> nano_ros_ret_t {
    if server.is_null() || storage.is_null() || capacity == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_UNINITIALIZED {
        return NANO_ROS_RET_ALREADY_EXISTS;
    }

    // Initialize storage to default values
    for i in 0..capacity {
        *storage.add(i) = nano_ros_parameter_t::default();
    }

    server.parameters = storage;
    server.capacity = capacity;
    server.count = 0;
    server.callback = None;
    server.callback_context = ptr::null_mut();
    server.state = nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY;

    NANO_ROS_RET_OK
}

/// Set a parameter change callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_server_set_callback(
    server: *mut nano_ros_param_server_t,
    callback: nano_ros_param_callback_t,
    context: *mut c_void,
) -> nano_ros_ret_t {
    if server.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    server.callback = callback;
    server.callback_context = context;

    NANO_ROS_RET_OK
}

/// Find a parameter by name. Returns the index or None if not found.
unsafe fn find_parameter(server: &nano_ros_param_server_t, name: *const c_char) -> Option<usize> {
    if name.is_null() || server.parameters.is_null() {
        return None;
    }

    for i in 0..server.count {
        let param = &*server.parameters.add(i);
        if cstr_eq_buffer(name, &param.name) {
            return Some(i);
        }
    }

    None
}

/// Internal function to declare a parameter.
unsafe fn declare_parameter_internal(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    param_type: nano_ros_parameter_type_t,
    value: nano_ros_parameter_value_t,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    // Check if parameter already exists
    if find_parameter(server, name).is_some() {
        return NANO_ROS_RET_ALREADY_EXISTS;
    }

    // Check capacity
    if server.count >= server.capacity {
        return NANO_ROS_RET_FULL;
    }

    // Add the parameter
    let param = &mut *server.parameters.add(server.count);
    copy_cstr_to_buffer(name, &mut param.name);
    param.r#type = param_type;
    param.value = value;
    server.count += 1;

    NANO_ROS_RET_OK
}

/// Declare a boolean parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_declare_bool(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    default_value: bool,
) -> nano_ros_ret_t {
    let value = nano_ros_parameter_value_t {
        bool_value: default_value,
    };
    declare_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_BOOL,
        value,
    )
}

/// Declare an integer parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_declare_integer(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    default_value: i64,
) -> nano_ros_ret_t {
    let value = nano_ros_parameter_value_t {
        integer_value: default_value,
    };
    declare_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_INTEGER,
        value,
    )
}

/// Declare a double parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_declare_double(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    default_value: f64,
) -> nano_ros_ret_t {
    let value = nano_ros_parameter_value_t {
        double_value: default_value,
    };
    declare_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_DOUBLE,
        value,
    )
}

/// Declare a string parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_declare_string(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    default_value: *const c_char,
) -> nano_ros_ret_t {
    if default_value.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let mut value = nano_ros_parameter_value_t {
        string_value: [0u8; NANO_ROS_MAX_PARAM_STRING_LEN],
    };
    copy_cstr_to_buffer(default_value, &mut value.string_value);

    declare_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_STRING,
        value,
    )
}

/// Get a boolean parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_get_bool(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
    value: *mut bool,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() || value.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &*server.parameters.add(idx);
            if param.r#type != nano_ros_parameter_type_t::NANO_ROS_PARAMETER_BOOL {
                return NANO_ROS_RET_INVALID_ARGUMENT;
            }
            *value = param.value.bool_value;
            NANO_ROS_RET_OK
        }
        None => NANO_ROS_RET_NOT_FOUND,
    }
}

/// Get an integer parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_get_integer(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
    value: *mut i64,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() || value.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &*server.parameters.add(idx);
            if param.r#type != nano_ros_parameter_type_t::NANO_ROS_PARAMETER_INTEGER {
                return NANO_ROS_RET_INVALID_ARGUMENT;
            }
            *value = param.value.integer_value;
            NANO_ROS_RET_OK
        }
        None => NANO_ROS_RET_NOT_FOUND,
    }
}

/// Get a double parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_get_double(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
    value: *mut f64,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() || value.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &*server.parameters.add(idx);
            if param.r#type != nano_ros_parameter_type_t::NANO_ROS_PARAMETER_DOUBLE {
                return NANO_ROS_RET_INVALID_ARGUMENT;
            }
            *value = param.value.double_value;
            NANO_ROS_RET_OK
        }
        None => NANO_ROS_RET_NOT_FOUND,
    }
}

/// Get a string parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_get_string(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
    value: *mut c_char,
    max_len: usize,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() || value.is_null() || max_len == 0 {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &*server.parameters.add(idx);
            if param.r#type != nano_ros_parameter_type_t::NANO_ROS_PARAMETER_STRING {
                return NANO_ROS_RET_INVALID_ARGUMENT;
            }

            // Copy string to output buffer
            let dst = core::slice::from_raw_parts_mut(value as *mut u8, max_len);
            let src = &param.value.string_value;
            let copy_len = max_len.min(NANO_ROS_MAX_PARAM_STRING_LEN) - 1;

            for i in 0..copy_len {
                if src[i] == 0 {
                    dst[i] = 0;
                    return NANO_ROS_RET_OK;
                }
                dst[i] = src[i];
            }
            dst[copy_len] = 0;

            NANO_ROS_RET_OK
        }
        None => NANO_ROS_RET_NOT_FOUND,
    }
}

/// Internal function to set a parameter value.
unsafe fn set_parameter_internal(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    param_type: nano_ros_parameter_type_t,
    new_value: nano_ros_parameter_value_t,
) -> nano_ros_ret_t {
    if server.is_null() || name.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return NANO_ROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &mut *server.parameters.add(idx);

            // Check type matches
            if param.r#type != param_type {
                return NANO_ROS_RET_INVALID_ARGUMENT;
            }

            // Create a temporary parameter for the callback
            let mut new_param = *param;
            new_param.value = new_value;

            // Call the callback if set
            if let Some(callback) = server.callback {
                let accepted = callback(
                    param.name.as_ptr() as *const c_char,
                    &new_param,
                    server.callback_context,
                );
                if !accepted {
                    return NANO_ROS_RET_ERROR;
                }
            }

            // Apply the change
            param.value = new_value;

            NANO_ROS_RET_OK
        }
        None => NANO_ROS_RET_NOT_FOUND,
    }
}

/// Set a boolean parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_set_bool(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    value: bool,
) -> nano_ros_ret_t {
    let new_value = nano_ros_parameter_value_t { bool_value: value };
    set_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_BOOL,
        new_value,
    )
}

/// Set an integer parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_set_integer(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    value: i64,
) -> nano_ros_ret_t {
    let new_value = nano_ros_parameter_value_t {
        integer_value: value,
    };
    set_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_INTEGER,
        new_value,
    )
}

/// Set a double parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_set_double(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    value: f64,
) -> nano_ros_ret_t {
    let new_value = nano_ros_parameter_value_t {
        double_value: value,
    };
    set_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_DOUBLE,
        new_value,
    )
}

/// Set a string parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_set_string(
    server: *mut nano_ros_param_server_t,
    name: *const c_char,
    value: *const c_char,
) -> nano_ros_ret_t {
    if value.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let mut new_value = nano_ros_parameter_value_t {
        string_value: [0u8; NANO_ROS_MAX_PARAM_STRING_LEN],
    };
    copy_cstr_to_buffer(value, &mut new_value.string_value);

    set_parameter_internal(
        server,
        name,
        nano_ros_parameter_type_t::NANO_ROS_PARAMETER_STRING,
        new_value,
    )
}

/// Check if a parameter exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_has(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
) -> bool {
    if server.is_null() || name.is_null() {
        return false;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return false;
    }

    find_parameter(server, name).is_some()
}

/// Get the type of a parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_get_type(
    server: *const nano_ros_param_server_t,
    name: *const c_char,
) -> nano_ros_parameter_type_t {
    if server.is_null() || name.is_null() {
        return nano_ros_parameter_type_t::NANO_ROS_PARAMETER_NOT_SET;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return nano_ros_parameter_type_t::NANO_ROS_PARAMETER_NOT_SET;
    }

    match find_parameter(server, name) {
        Some(idx) => (*server.parameters.add(idx)).r#type,
        None => nano_ros_parameter_type_t::NANO_ROS_PARAMETER_NOT_SET,
    }
}

/// Get the number of declared parameters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_server_get_count(
    server: *const nano_ros_param_server_t,
) -> usize {
    if server.is_null() {
        return 0;
    }

    let server = &*server;

    if server.state != nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_READY {
        return 0;
    }

    server.count
}

/// Finalize a parameter server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nano_ros_param_server_fini(
    server: *mut nano_ros_param_server_t,
) -> nano_ros_ret_t {
    if server.is_null() {
        return NANO_ROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state == nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_UNINITIALIZED {
        return NANO_ROS_RET_NOT_INIT;
    }

    server.state = nano_ros_param_server_state_t::NANO_ROS_PARAM_SERVER_STATE_SHUTDOWN;
    server.parameters = ptr::null_mut();
    server.capacity = 0;
    server.count = 0;
    server.callback = None;
    server.callback_context = ptr::null_mut();

    NANO_ROS_RET_OK
}
