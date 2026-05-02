//! Parameter API for nros C API.
//!
//! Provides node parameters for configuration and runtime tuning.

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use crate::error::*;

// ============================================================================
// Constants
// ============================================================================

/// Maximum length of a parameter name
pub const NROS_MAX_PARAM_NAME_LEN: usize = 64;

/// Maximum length of a string parameter value
pub const NROS_MAX_PARAM_STRING_LEN: usize = 128;

// ============================================================================
// Parameter Types
// ============================================================================

/// Parameter type enumeration.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_parameter_type_t {
    /// Parameter not set
    NROS_PARAMETER_NOT_SET = 0,
    /// Boolean parameter
    NROS_PARAMETER_BOOL = 1,
    /// 64-bit signed integer parameter
    NROS_PARAMETER_INTEGER = 2,
    /// 64-bit floating point parameter
    NROS_PARAMETER_DOUBLE = 3,
    /// String parameter
    NROS_PARAMETER_STRING = 4,
    /// Byte array parameter
    NROS_PARAMETER_BYTE_ARRAY = 5,
    /// Boolean array parameter
    NROS_PARAMETER_BOOL_ARRAY = 6,
    /// Integer array parameter
    NROS_PARAMETER_INTEGER_ARRAY = 7,
    /// Double array parameter
    NROS_PARAMETER_DOUBLE_ARRAY = 8,
    /// String array parameter
    NROS_PARAMETER_STRING_ARRAY = 9,
}

/// Array parameter value (pointer + length to caller-owned data).
///
/// The caller must keep the array data valid for the lifetime of the parameter.
/// For string arrays, `data` points to an array of `*const c_char` pointers.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct nros_param_array_t {
    /// Pointer to caller-owned array data
    pub data: *const c_void,
    /// Number of elements
    pub len: usize,
}

/// Parameter value union.
#[repr(C)]
#[derive(Clone, Copy)]
pub union nros_parameter_value_t {
    /// Boolean value
    pub bool_value: bool,
    /// Integer value (64-bit)
    pub integer_value: i64,
    /// Double value
    pub double_value: f64,
    /// String value (fixed-size buffer)
    pub string_value: [u8; NROS_MAX_PARAM_STRING_LEN],
    /// Array value (pointer + length)
    pub array_value: nros_param_array_t,
}

impl Default for nros_parameter_value_t {
    fn default() -> Self {
        Self { integer_value: 0 }
    }
}

/// Parameter structure.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nros_parameter_t {
    /// Parameter name (null-terminated)
    pub name: [u8; NROS_MAX_PARAM_NAME_LEN],
    /// Parameter type
    pub r#type: nros_parameter_type_t,
    /// Parameter value
    pub value: nros_parameter_value_t,
}

impl Default for nros_parameter_t {
    fn default() -> Self {
        Self {
            name: [0u8; NROS_MAX_PARAM_NAME_LEN],
            r#type: nros_parameter_type_t::NROS_PARAMETER_NOT_SET,
            value: nros_parameter_value_t::default(),
        }
    }
}

// ============================================================================
// Parameter Server
// ============================================================================

/// Parameter server state.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum nros_param_server_state_t {
    /// Not initialized
    NROS_PARAM_SERVER_STATE_UNINITIALIZED = 0,
    /// Initialized and ready
    NROS_PARAM_SERVER_STATE_READY = 1,
    /// Shutdown
    NROS_PARAM_SERVER_STATE_SHUTDOWN = 2,
}

/// Parameter change callback type.
pub type nros_param_callback_t = Option<
    unsafe extern "C" fn(
        name: *const c_char,
        param: *const nros_parameter_t,
        context: *mut c_void,
    ) -> bool,
>;

/// Parameter server structure.
#[repr(C)]
pub struct nros_param_server_t {
    /// Current state
    pub state: nros_param_server_state_t,
    /// Maximum number of parameters
    pub capacity: usize,
    /// Current number of parameters
    pub count: usize,
    /// Parameter storage (pointer to user-provided array)
    parameters: *mut nros_parameter_t,
    /// Parameter change callback
    callback: nros_param_callback_t,
    /// Callback context
    callback_context: *mut c_void,
}

impl Default for nros_param_server_t {
    fn default() -> Self {
        Self {
            state: nros_param_server_state_t::NROS_PARAM_SERVER_STATE_UNINITIALIZED,
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
pub extern "C" fn nros_param_server_get_zero_initialized() -> nros_param_server_t {
    nros_param_server_t::default()
}

/// Initialize a parameter server with user-provided storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_server_init(
    server: *mut nros_param_server_t,
    storage: *mut nros_parameter_t,
    capacity: usize,
) -> nros_ret_t {
    if server.is_null() || storage.is_null() || capacity == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_UNINITIALIZED {
        return NROS_RET_ALREADY_EXISTS;
    }

    // Initialize storage to default values
    for i in 0..capacity {
        *storage.add(i) = nros_parameter_t::default();
    }

    server.parameters = storage;
    server.capacity = capacity;
    server.count = 0;
    server.callback = None;
    server.callback_context = ptr::null_mut();
    server.state = nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY;

    NROS_RET_OK
}

/// Set a parameter change callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_server_set_callback(
    server: *mut nros_param_server_t,
    callback: nros_param_callback_t,
    context: *mut c_void,
) -> nros_ret_t {
    if server.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    server.callback = callback;
    server.callback_context = context;

    NROS_RET_OK
}

/// Find a parameter by name. Returns the index or None if not found.
unsafe fn find_parameter(server: &nros_param_server_t, name: *const c_char) -> Option<usize> {
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
    server: *mut nros_param_server_t,
    name: *const c_char,
    param_type: nros_parameter_type_t,
    value: nros_parameter_value_t,
) -> nros_ret_t {
    if server.is_null() || name.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    // Check if parameter already exists
    if find_parameter(server, name).is_some() {
        return NROS_RET_ALREADY_EXISTS;
    }

    // Check capacity
    if server.count >= server.capacity {
        return NROS_RET_FULL;
    }

    // Add the parameter
    let param = &mut *server.parameters.add(server.count);
    copy_cstr_to_buffer(name, &mut param.name);
    param.r#type = param_type;
    param.value = value;
    server.count += 1;

    NROS_RET_OK
}

/// Generate `declare`/`get`/`set` FFI functions for a scalar parameter type.
macro_rules! impl_param_scalar {
    (
        name: $name:ident,
        ty: $T:ty,
        variant: $variant:ident,
        union_field: $field:ident,
        doc: $doc:literal
    ) => {
        paste::paste! {
            #[doc = "Declare " $doc " parameter."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_declare_ $name>](
                server: *mut nros_param_server_t,
                name: *const c_char,
                default_value: $T,
            ) -> nros_ret_t {
                let value = nros_parameter_value_t { $field: default_value };
                declare_parameter_internal(
                    server,
                    name,
                    nros_parameter_type_t::[<NROS_PARAMETER_ $variant>],
                    value,
                )
            }

            #[doc = "Get " $doc " parameter value."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_get_ $name>](
                server: *const nros_param_server_t,
                name: *const c_char,
                value: *mut $T,
            ) -> nros_ret_t {
                if server.is_null() || name.is_null() || value.is_null() {
                    return NROS_RET_INVALID_ARGUMENT;
                }
                let server = &*server;
                if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
                    return NROS_RET_NOT_INIT;
                }
                match find_parameter(server, name) {
                    Some(idx) => {
                        let param = &*server.parameters.add(idx);
                        if param.r#type != nros_parameter_type_t::[<NROS_PARAMETER_ $variant>] {
                            return NROS_RET_INVALID_ARGUMENT;
                        }
                        *value = param.value.$field;
                        NROS_RET_OK
                    }
                    None => NROS_RET_NOT_FOUND,
                }
            }

            #[doc = "Set " $doc " parameter value."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_set_ $name>](
                server: *mut nros_param_server_t,
                name: *const c_char,
                value: $T,
            ) -> nros_ret_t {
                let new_value = nros_parameter_value_t { $field: value };
                set_parameter_internal(
                    server,
                    name,
                    nros_parameter_type_t::[<NROS_PARAMETER_ $variant>],
                    new_value,
                )
            }
        }
    };
}

impl_param_scalar!(name: bool, ty: bool, variant: BOOL, union_field: bool_value, doc: "a boolean");
impl_param_scalar!(name: integer, ty: i64, variant: INTEGER, union_field: integer_value, doc: "an integer");
impl_param_scalar!(name: double, ty: f64, variant: DOUBLE, union_field: double_value, doc: "a double");

/// Declare a string parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_declare_string(
    server: *mut nros_param_server_t,
    name: *const c_char,
    default_value: *const c_char,
) -> nros_ret_t {
    if default_value.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let mut value = nros_parameter_value_t {
        string_value: [0u8; NROS_MAX_PARAM_STRING_LEN],
    };
    copy_cstr_to_buffer(default_value, &mut value.string_value);

    declare_parameter_internal(
        server,
        name,
        nros_parameter_type_t::NROS_PARAMETER_STRING,
        value,
    )
}

/// Get a string parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_get_string(
    server: *const nros_param_server_t,
    name: *const c_char,
    value: *mut c_char,
    max_len: usize,
) -> nros_ret_t {
    if server.is_null() || name.is_null() || value.is_null() || max_len == 0 {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &*server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &*server.parameters.add(idx);
            if param.r#type != nros_parameter_type_t::NROS_PARAMETER_STRING {
                return NROS_RET_INVALID_ARGUMENT;
            }

            // Copy string to output buffer
            let dst = core::slice::from_raw_parts_mut(value as *mut u8, max_len);
            let src = &param.value.string_value;
            let copy_len = max_len.min(NROS_MAX_PARAM_STRING_LEN) - 1;

            for i in 0..copy_len {
                if src[i] == 0 {
                    dst[i] = 0;
                    return NROS_RET_OK;
                }
                dst[i] = src[i];
            }
            dst[copy_len] = 0;

            NROS_RET_OK
        }
        None => NROS_RET_NOT_FOUND,
    }
}

/// Internal function to set a parameter value.
unsafe fn set_parameter_internal(
    server: *mut nros_param_server_t,
    name: *const c_char,
    param_type: nros_parameter_type_t,
    new_value: nros_parameter_value_t,
) -> nros_ret_t {
    if server.is_null() || name.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return NROS_RET_NOT_INIT;
    }

    match find_parameter(server, name) {
        Some(idx) => {
            let param = &mut *server.parameters.add(idx);

            // Check type matches
            if param.r#type != param_type {
                return NROS_RET_INVALID_ARGUMENT;
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
                    return NROS_RET_ERROR;
                }
            }

            // Apply the change
            param.value = new_value;

            NROS_RET_OK
        }
        None => NROS_RET_NOT_FOUND,
    }
}

/// Set a string parameter value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_set_string(
    server: *mut nros_param_server_t,
    name: *const c_char,
    value: *const c_char,
) -> nros_ret_t {
    if value.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let mut new_value = nros_parameter_value_t {
        string_value: [0u8; NROS_MAX_PARAM_STRING_LEN],
    };
    copy_cstr_to_buffer(value, &mut new_value.string_value);

    set_parameter_internal(
        server,
        name,
        nros_parameter_type_t::NROS_PARAMETER_STRING,
        new_value,
    )
}

// ============================================================================
// Array Parameter Functions
// ============================================================================

/// Macro to generate declare/get/set functions for array parameter types.
///
/// For each array type, generates three `#[unsafe(no_mangle)]` extern "C" functions:
/// - `nros_param_declare_{name}_array`: Declare with initial data
/// - `nros_param_get_{name}_array`: Get stored pointer + length
/// - `nros_param_set_{name}_array`: Update stored pointer + length
macro_rules! impl_param_array {
    (
        name: $name:ident,
        elem: $T:ty,
        variant: $variant:ident,
        doc: $doc:literal
    ) => {
        paste::paste! {
            #[doc = "Declare " $doc " parameter.\n\n"]
            #[doc = "The caller must keep the array data valid for the lifetime of the parameter.\n"]
            #[doc = "`data` may be NULL only if `len` is 0."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_declare_ $name _array>](
                server: *mut nros_param_server_t,
                name: *const c_char,
                data: *const $T,
                len: usize,
            ) -> nros_ret_t {
                if data.is_null() && len != 0 {
                    return NROS_RET_INVALID_ARGUMENT;
                }
                let value = nros_parameter_value_t {
                    array_value: nros_param_array_t {
                        data: data as *const c_void,
                        len,
                    },
                };
                declare_parameter_internal(
                    server,
                    name,
                    nros_parameter_type_t::[<NROS_PARAMETER_ $variant>],
                    value,
                )
            }

            #[doc = "Get " $doc " parameter value.\n\n"]
            #[doc = "Returns the stored pointer and element count via out-parameters."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_get_ $name _array>](
                server: *const nros_param_server_t,
                name: *const c_char,
                data: *mut *const $T,
                len: *mut usize,
            ) -> nros_ret_t {
                if server.is_null() || name.is_null() || data.is_null() || len.is_null() {
                    return NROS_RET_INVALID_ARGUMENT;
                }

                let server = &*server;

                if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
                    return NROS_RET_NOT_INIT;
                }

                match find_parameter(server, name) {
                    Some(idx) => {
                        let param = &*server.parameters.add(idx);
                        if param.r#type
                            != nros_parameter_type_t::[<NROS_PARAMETER_ $variant>]
                        {
                            return NROS_RET_INVALID_ARGUMENT;
                        }
                        *data = param.value.array_value.data as *const $T;
                        *len = param.value.array_value.len;
                        NROS_RET_OK
                    }
                    None => NROS_RET_NOT_FOUND,
                }
            }

            #[doc = "Set " $doc " parameter value.\n\n"]
            #[doc = "The caller must keep the array data valid for the lifetime of the parameter.\n"]
            #[doc = "`data` may be NULL only if `len` is 0."]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn [<nros_param_set_ $name _array>](
                server: *mut nros_param_server_t,
                name: *const c_char,
                data: *const $T,
                len: usize,
            ) -> nros_ret_t {
                if data.is_null() && len != 0 {
                    return NROS_RET_INVALID_ARGUMENT;
                }
                let new_value = nros_parameter_value_t {
                    array_value: nros_param_array_t {
                        data: data as *const c_void,
                        len,
                    },
                };
                set_parameter_internal(
                    server,
                    name,
                    nros_parameter_type_t::[<NROS_PARAMETER_ $variant>],
                    new_value,
                )
            }
        }
    };
}

impl_param_array!(name: byte, elem: u8, variant: BYTE_ARRAY, doc: "a byte array");
impl_param_array!(name: bool, elem: bool, variant: BOOL_ARRAY, doc: "a boolean array");
impl_param_array!(name: integer, elem: i64, variant: INTEGER_ARRAY, doc: "an integer array");
impl_param_array!(name: double, elem: f64, variant: DOUBLE_ARRAY, doc: "a double array");
impl_param_array!(name: string, elem: *const c_char, variant: STRING_ARRAY, doc: "a string array");

// ============================================================================
// Query Functions
// ============================================================================

/// Check if a parameter exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_has(
    server: *const nros_param_server_t,
    name: *const c_char,
) -> bool {
    if server.is_null() || name.is_null() {
        return false;
    }

    let server = &*server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return false;
    }

    find_parameter(server, name).is_some()
}

/// Get the type of a parameter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_get_type(
    server: *const nros_param_server_t,
    name: *const c_char,
) -> nros_parameter_type_t {
    if server.is_null() || name.is_null() {
        return nros_parameter_type_t::NROS_PARAMETER_NOT_SET;
    }

    let server = &*server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return nros_parameter_type_t::NROS_PARAMETER_NOT_SET;
    }

    match find_parameter(server, name) {
        Some(idx) => (*server.parameters.add(idx)).r#type,
        None => nros_parameter_type_t::NROS_PARAMETER_NOT_SET,
    }
}

/// Get the number of declared parameters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_server_get_count(server: *const nros_param_server_t) -> usize {
    if server.is_null() {
        return 0;
    }

    let server = &*server;

    if server.state != nros_param_server_state_t::NROS_PARAM_SERVER_STATE_READY {
        return 0;
    }

    server.count
}

/// Finalize a parameter server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_param_server_fini(server: *mut nros_param_server_t) -> nros_ret_t {
    if server.is_null() {
        return NROS_RET_INVALID_ARGUMENT;
    }

    let server = &mut *server;

    if server.state == nros_param_server_state_t::NROS_PARAM_SERVER_STATE_UNINITIALIZED {
        return NROS_RET_NOT_INIT;
    }

    server.state = nros_param_server_state_t::NROS_PARAM_SERVER_STATE_SHUTDOWN;
    server.parameters = ptr::null_mut();
    server.capacity = 0;
    server.count = 0;
    server.callback = None;
    server.callback_context = ptr::null_mut();

    NROS_RET_OK
}

// ============================================================================
// Service-Backed Parameter API (Phase 84.B3)
// ============================================================================
//
// These functions operate on the `nros-params::ParameterServer` owned by
// the Executor. Unlike the legacy `nros_param_server_t` API above, a
// parameter declared here is visible to `ros2 param list /<node>` once
// `nros_executor_register_parameter_services` is called.
//
// The RMW backend must be compiled in (`rmw-zenoh` or `rmw-xrce`) because
// the Executor type is only defined under those features.

#[cfg(all(
    feature = "param-services",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")
))]
mod service_backed {
    use super::*;
    use crate::executor::{get_executor, nros_executor_t};
    use nros_node::{ParameterValue, SetParameterResult};

    /// Read a C string up to `max_len` bytes into a `&str` slice.
    /// Returns `None` if `name` is NULL or non-UTF-8.
    unsafe fn cstr_to_str<'a>(name: *const c_char) -> Option<&'a str> {
        if name.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *name.add(len) != 0 {
            len += 1;
            if len > NROS_MAX_PARAM_NAME_LEN {
                return None;
            }
        }
        core::str::from_utf8(core::slice::from_raw_parts(name as *const u8, len)).ok()
    }

    /// Copy a `&str` to a null-terminated C buffer of size `max_len`.
    /// Returns `NROS_RET_INVALID_ARGUMENT` if the buffer is too small.
    unsafe fn str_to_cbuf(src: &str, dst: *mut c_char, max_len: usize) -> nros_ret_t {
        if max_len == 0 {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let bytes = src.as_bytes();
        if bytes.len() + 1 > max_len {
            return NROS_RET_INVALID_ARGUMENT;
        }
        for (i, &b) in bytes.iter().enumerate() {
            *dst.add(i) = b as c_char;
        }
        *dst.add(bytes.len()) = 0;
        NROS_RET_OK
    }

    /// Register the 6 ROS 2 parameter services on the executor's node.
    ///
    /// After this call, parameters declared via
    /// `nros_executor_declare_param_*` are visible to `ros2 param list`,
    /// `ros2 param get`, `ros2 param set`, etc.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_register_parameter_services(
        executor: *mut nros_executor_t,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let exec = get_executor(&mut (*executor)._opaque);
        match exec.register_parameter_services() {
            Ok(()) => NROS_RET_OK,
            Err(_) => NROS_RET_ERROR,
        }
    }

    macro_rules! impl_executor_param_scalar {
        (
            name: $name:ident,
            ty: $T:ty,
            from_variant: $from:path,
            as_variant: $as:ident,
            doc: $doc:literal
        ) => {
            paste::paste! {
                #[doc = "Declare " $doc " parameter on the executor's server."]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn [<nros_executor_declare_param_ $name>](
                    executor: *mut nros_executor_t,
                    name: *const c_char,
                    value: $T,
                ) -> nros_ret_t {
                    if executor.is_null() {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                    let Some(n) = cstr_to_str(name) else {
                        return NROS_RET_INVALID_ARGUMENT;
                    };
                    let exec = get_executor(&mut (*executor)._opaque);
                    if exec.declare_parameter(n, $from(value)) {
                        NROS_RET_OK
                    } else {
                        NROS_RET_ALREADY_EXISTS
                    }
                }

                #[doc = "Get " $doc " parameter from the executor's server."]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn [<nros_executor_get_param_ $name>](
                    executor: *mut nros_executor_t,
                    name: *const c_char,
                    out_value: *mut $T,
                ) -> nros_ret_t {
                    if executor.is_null() || out_value.is_null() {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                    let Some(n) = cstr_to_str(name) else {
                        return NROS_RET_INVALID_ARGUMENT;
                    };
                    let inner = get_executor(&mut (*executor)._opaque);
                    match inner.get_parameter(n).and_then(|v| v.$as()) {
                        Some(v) => {
                            *out_value = v.into();
                            NROS_RET_OK
                        }
                        None => NROS_RET_NOT_FOUND,
                    }
                }

                #[doc = "Set " $doc " parameter on the executor's server."]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn [<nros_executor_set_param_ $name>](
                    executor: *mut nros_executor_t,
                    name: *const c_char,
                    value: $T,
                ) -> nros_ret_t {
                    if executor.is_null() {
                        return NROS_RET_INVALID_ARGUMENT;
                    }
                    let Some(n) = cstr_to_str(name) else {
                        return NROS_RET_INVALID_ARGUMENT;
                    };
                    let exec = get_executor(&mut (*executor)._opaque);
                    let Some(server) = exec.params_mut() else {
                        return NROS_RET_NOT_INIT;
                    };
                    match server.set(n, $from(value)) {
                        SetParameterResult::Success => NROS_RET_OK,
                        SetParameterResult::NotFound => NROS_RET_NOT_FOUND,
                        _ => NROS_RET_INVALID_ARGUMENT,
                    }
                }
            }
        };
    }

    impl_executor_param_scalar!(
        name: bool, ty: bool,
        from_variant: ParameterValue::from_bool,
        as_variant: as_bool,
        doc: "a boolean"
    );
    impl_executor_param_scalar!(
        name: integer, ty: i64,
        from_variant: ParameterValue::from_integer,
        as_variant: as_integer,
        doc: "an integer"
    );
    impl_executor_param_scalar!(
        name: double, ty: f64,
        from_variant: ParameterValue::from_double,
        as_variant: as_double,
        doc: "a double"
    );

    /// Declare a string parameter on the executor's server.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_declare_param_string(
        executor: *mut nros_executor_t,
        name: *const c_char,
        value: *const c_char,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let (Some(n), Some(v)) = (cstr_to_str(name), cstr_to_str(value)) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_string(v) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        let exec = get_executor(&mut (*executor)._opaque);
        if exec.declare_parameter(n, pv) {
            NROS_RET_OK
        } else {
            NROS_RET_ALREADY_EXISTS
        }
    }

    /// Get a string parameter from the executor's server into a fixed buffer.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_get_param_string(
        executor: *mut nros_executor_t,
        name: *const c_char,
        out_value: *mut c_char,
        max_len: usize,
    ) -> nros_ret_t {
        if executor.is_null() || out_value.is_null() || max_len == 0 {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let Some(n) = cstr_to_str(name) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        let inner = get_executor(&mut (*executor)._opaque);
        match inner.get_parameter(n).and_then(|v| v.as_string()) {
            Some(s) => str_to_cbuf(s, out_value, max_len),
            None => NROS_RET_NOT_FOUND,
        }
    }

    /// Set a string parameter on the executor's server.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_set_param_string(
        executor: *mut nros_executor_t,
        name: *const c_char,
        value: *const c_char,
    ) -> nros_ret_t {
        if executor.is_null() {
            return NROS_RET_INVALID_ARGUMENT;
        }
        let (Some(n), Some(v)) = (cstr_to_str(name), cstr_to_str(value)) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_string(v) else {
            return NROS_RET_INVALID_ARGUMENT;
        };
        let exec = get_executor(&mut (*executor)._opaque);
        let Some(server) = exec.params_mut() else {
            return NROS_RET_NOT_INIT;
        };
        match server.set(n, pv) {
            SetParameterResult::Success => NROS_RET_OK,
            SetParameterResult::NotFound => NROS_RET_NOT_FOUND,
            _ => NROS_RET_INVALID_ARGUMENT,
        }
    }

    /// Check if a parameter exists on the executor's server.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn nros_executor_has_param(
        executor: *mut nros_executor_t,
        name: *const c_char,
    ) -> bool {
        if executor.is_null() {
            return false;
        }
        let Some(n) = cstr_to_str(name) else {
            return false;
        };
        let inner = get_executor(&mut (*executor)._opaque);
        inner.get_parameter(n).is_some()
    }
}

#[cfg(all(
    feature = "param-services",
    any(feature = "rmw-zenoh", feature = "rmw-xrce", feature = "rmw-dds")
))]
pub use service_backed::*;

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_char;

    /// Helper to create an initialized parameter server with storage.
    unsafe fn setup_server(server: &mut nros_param_server_t, storage: &mut [nros_parameter_t]) {
        *server = nros_param_server_get_zero_initialized();
        let ret = nros_param_server_init(server, storage.as_mut_ptr(), storage.len());
        assert_eq!(ret, NROS_RET_OK);
    }

    #[test]
    fn test_declare_and_get_integer_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [i64; 3] = [10, 20, 30];
            let name = c"int_arr".as_ptr();
            let ret =
                nros_param_declare_integer_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const i64 = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_integer_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 3);
            assert_eq!(*data, 10);
            assert_eq!(*data.add(1), 20);
            assert_eq!(*data.add(2), 30);
        }
    }

    #[test]
    fn test_declare_and_get_double_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [f64; 2] = [1.5, 2.5];
            let name = c"dbl_arr".as_ptr();
            let ret =
                nros_param_declare_double_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const f64 = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_double_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 2);
            assert_eq!(*data, 1.5);
            assert_eq!(*data.add(1), 2.5);
        }
    }

    #[test]
    fn test_declare_and_get_bool_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [bool; 3] = [true, false, true];
            let name = c"bool_arr".as_ptr();
            let ret =
                nros_param_declare_bool_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const bool = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_bool_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 3);
            assert!(*data);
            assert!(!*data.add(1));
            assert!(*data.add(2));
        }
    }

    #[test]
    fn test_declare_and_get_byte_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];
            let name = c"byte_arr".as_ptr();
            let ret =
                nros_param_declare_byte_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const u8 = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_byte_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 4);
            assert_eq!(
                core::slice::from_raw_parts(data, len),
                &[0xDE, 0xAD, 0xBE, 0xEF]
            );
        }
    }

    #[test]
    fn test_declare_and_get_string_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let s1 = c"hello".as_ptr();
            let s2 = c"world".as_ptr();
            let strings: [*const c_char; 2] = [s1, s2];
            let name = c"str_arr".as_ptr();
            let ret =
                nros_param_declare_string_array(&mut server, name, strings.as_ptr(), strings.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const *const c_char = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_string_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 2);
            assert_eq!(*data, s1);
            assert_eq!(*data.add(1), s2);
        }
    }

    #[test]
    fn test_set_integer_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let initial: [i64; 2] = [1, 2];
            let name = c"int_arr".as_ptr();
            let ret = nros_param_declare_integer_array(
                &mut server,
                name,
                initial.as_ptr(),
                initial.len(),
            );
            assert_eq!(ret, NROS_RET_OK);

            let updated: [i64; 3] = [10, 20, 30];
            let ret =
                nros_param_set_integer_array(&mut server, name, updated.as_ptr(), updated.len());
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const i64 = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_integer_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 3);
            assert_eq!(*data, 10);
        }
    }

    #[test]
    fn test_empty_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let name = c"empty".as_ptr();
            let ret = nros_param_declare_integer_array(&mut server, name, ptr::null(), 0);
            assert_eq!(ret, NROS_RET_OK);

            let mut data: *const i64 = ptr::null();
            let mut len: usize = 99;
            let ret = nros_param_get_integer_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_OK);
            assert_eq!(len, 0);
        }
    }

    #[test]
    fn test_null_data_nonzero_len_rejected() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let name = c"bad".as_ptr();
            let ret = nros_param_declare_integer_array(&mut server, name, ptr::null(), 5);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);

            // Also test set path
            let initial: [i64; 1] = [1];
            let ret = nros_param_declare_integer_array(
                &mut server,
                name,
                initial.as_ptr(),
                initial.len(),
            );
            assert_eq!(ret, NROS_RET_OK);

            let ret = nros_param_set_integer_array(&mut server, name, ptr::null(), 3);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_type_mismatch_array_get() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [i64; 2] = [1, 2];
            let name = c"int_arr".as_ptr();
            let ret =
                nros_param_declare_integer_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            // Try to get as double array
            let mut data: *const f64 = ptr::null();
            let mut len: usize = 0;
            let ret = nros_param_get_double_array(&server, name, &mut data, &mut len);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_type_mismatch_set_scalar_on_array() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [i64; 2] = [1, 2];
            let name = c"int_arr".as_ptr();
            let ret =
                nros_param_declare_integer_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            // Try to set as scalar integer
            let ret = nros_param_set_integer(&mut server, name, 42);
            assert_eq!(ret, NROS_RET_INVALID_ARGUMENT);
        }
    }

    #[test]
    fn test_get_type_returns_array_type() {
        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let values: [f64; 1] = [3.14];
            let name = c"dbl_arr".as_ptr();
            let ret =
                nros_param_declare_double_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            let ptype = nros_param_get_type(&server, name);
            assert_eq!(ptype, nros_parameter_type_t::NROS_PARAMETER_DOUBLE_ARRAY);
        }
    }

    #[test]
    fn test_callback_with_array_param() {
        static mut CALLBACK_CALLED: bool = false;

        unsafe extern "C" fn on_change(
            _name: *const c_char,
            _param: *const nros_parameter_t,
            _context: *mut c_void,
        ) -> bool {
            unsafe {
                CALLBACK_CALLED = true;
            }
            true
        }

        let mut server = nros_param_server_t::default();
        let mut storage = [nros_parameter_t::default(); 4];
        unsafe {
            setup_server(&mut server, &mut storage);

            let ret = nros_param_server_set_callback(&mut server, Some(on_change), ptr::null_mut());
            assert_eq!(ret, NROS_RET_OK);

            let values: [i64; 2] = [1, 2];
            let name = c"int_arr".as_ptr();
            let ret =
                nros_param_declare_integer_array(&mut server, name, values.as_ptr(), values.len());
            assert_eq!(ret, NROS_RET_OK);

            CALLBACK_CALLED = false;
            let updated: [i64; 1] = [99];
            let ret =
                nros_param_set_integer_array(&mut server, name, updated.as_ptr(), updated.len());
            assert_eq!(ret, NROS_RET_OK);
            assert!(CALLBACK_CALLED);
        }
    }
}
