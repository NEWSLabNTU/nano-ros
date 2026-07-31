//! Phase 269 (W0) — executor-shim: parameter-service FFI over the CppContext handle.
//!
//! Mirrors `nros-c/src/parameter.rs`'s executor-backed functions but recovers the
//! executor from `CppContext*` instead of `nros_executor_t*`. W1 emitters call these;
//! no emitter wires them yet this wave.

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use core::ffi::{c_char, c_void};

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use nros_node::ParameterValue;

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use crate::{
    CppContext, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL, NROS_CPP_RET_INVALID_ARGUMENT,
    NROS_CPP_RET_NOT_FOUND, NROS_CPP_RET_OK, cstr_to_str, nros_cpp_ret_t,
};

/// Register the ROS 2 parameter services on the C++ executor's node.
///
/// After this call, `ros2 param list|get|set` can inspect and modify parameters.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_register_parameter_services(
    executor: *mut c_void,
) -> nros_cpp_ret_t {
    if executor.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    match ctx.executor.register_parameter_services() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Declare a parameter with a string initial value on the C++ executor's node.
///
/// Infers the `ParameterValue` type from the string content: booleans, integers,
/// floats, and plain strings are all handled (in that priority order). Mirrors the
/// Rust `nros::main!` W4b inference path.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` and `value` must be
/// valid null-terminated UTF-8 strings.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_declare_param(
    executor: *mut c_void,
    name: *const c_char,
    value: *const c_char,
) -> nros_cpp_ret_t {
    if executor.is_null() || name.is_null() || value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let val_str = match unsafe { cstr_to_str(value) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let pv = infer_param_value(val_str);
    if ctx.executor.declare_parameter(name_str, pv) {
        NROS_CPP_RET_OK
    } else {
        NROS_CPP_RET_ERROR
    }
}

/// Get an integer parameter by name from the C++ executor's parameter store.
///
/// Writes to `*out_value` on success. Returns `NROS_CPP_RET_OK` if found,
/// `NROS_CPP_RET_NOT_FOUND` if absent or wrong type, `NROS_CPP_RET_INVALID_ARGUMENT`
/// for null pointers.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` must be valid null-terminated
/// UTF-8. `out_value` must be valid and writable.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_get_param_integer(
    executor: *mut c_void,
    name: *const c_char,
    out_value: *mut i64,
) -> nros_cpp_ret_t {
    if executor.is_null() || out_value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    match ctx
        .executor
        .get_parameter(name_str)
        .and_then(|v| v.as_integer())
    {
        Some(v) => {
            unsafe { *out_value = v }
            NROS_CPP_RET_OK
        }
        None => NROS_CPP_RET_NOT_FOUND,
    }
}

/// Get a double parameter by name from the C++ executor's parameter store.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` must be valid null-terminated
/// UTF-8. `out_value` must be valid and writable.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_get_param_double(
    executor: *mut c_void,
    name: *const c_char,
    out_value: *mut f64,
) -> nros_cpp_ret_t {
    if executor.is_null() || out_value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    match ctx
        .executor
        .get_parameter(name_str)
        .and_then(|v| v.as_double())
    {
        Some(v) => {
            unsafe { *out_value = v }
            NROS_CPP_RET_OK
        }
        None => NROS_CPP_RET_NOT_FOUND,
    }
}

/// Get a string parameter by name from the C++ executor's parameter store.
///
/// Copies the value into `out_buf` (null-terminated). Returns `NROS_CPP_RET_FULL`
/// if the buffer is too small (string is truncated + null-terminated), or
/// `NROS_CPP_RET_NOT_FOUND` if the param is absent or wrong type.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` must be valid null-terminated
/// UTF-8. `out_buf` must be valid for `buf_len` bytes.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_get_param_string(
    executor: *mut c_void,
    name: *const c_char,
    out_buf: *mut c_char,
    buf_len: usize,
) -> nros_cpp_ret_t {
    if executor.is_null() || out_buf.is_null() || buf_len == 0 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let ctx = unsafe { &mut *(executor as *mut CppContext) };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let val = match ctx
        .executor
        .get_parameter(name_str)
        .and_then(|v| v.as_string())
    {
        Some(s) => s,
        None => return NROS_CPP_RET_NOT_FOUND,
    };
    let bytes = val.as_bytes();
    let copy_len = bytes.len().min(buf_len - 1);
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, out_buf, copy_len);
        *out_buf.add(copy_len) = 0;
    }
    if bytes.len() >= buf_len {
        NROS_CPP_RET_FULL
    } else {
        NROS_CPP_RET_OK
    }
}

/// Infer a `ParameterValue` from a raw launch-param string, mirroring the
/// Rust `nros::main!` W4b inference in `nros/src/node_runtime.rs::infer_param_value`.
///
/// Precedence: bool ("true"/"false") → integer → float → string (truncated if too long).
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
fn infer_param_value(raw: &str) -> ParameterValue {
    match raw {
        "true" | "True" | "TRUE" => return ParameterValue::from_bool(true),
        "false" | "False" | "FALSE" => return ParameterValue::from_bool(false),
        _ => {}
    }
    if let Ok(i) = raw.parse::<i64>() {
        return ParameterValue::from_integer(i);
    }
    if let Ok(f) = raw.parse::<f64>() {
        return ParameterValue::from_double(f);
    }
    ParameterValue::from_string(raw).unwrap_or(ParameterValue::NotSet)
}

#[cfg(test)]
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
mod tests {
    use core::ptr;

    use super::*;

    /// Null-pointer guard: every shim fn returns INVALID_ARGUMENT for a null executor.
    #[test]
    fn null_executor_returns_invalid_argument() {
        let ret = unsafe { nros_cpp_register_parameter_services(ptr::null_mut()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let name = c"p";
        let val = c"v";
        let ret = unsafe { nros_cpp_declare_param(ptr::null_mut(), name.as_ptr(), val.as_ptr()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn infer_param_value_bool() {
        assert_eq!(infer_param_value("true"), ParameterValue::from_bool(true));
        assert_eq!(infer_param_value("false"), ParameterValue::from_bool(false));
        assert_eq!(infer_param_value("True"), ParameterValue::from_bool(true));
        assert_eq!(infer_param_value("FALSE"), ParameterValue::from_bool(false));
    }

    #[test]
    fn infer_param_value_integer() {
        assert_eq!(infer_param_value("42"), ParameterValue::from_integer(42));
        assert_eq!(infer_param_value("-7"), ParameterValue::from_integer(-7));
    }

    #[test]
    fn infer_param_value_double() {
        assert_eq!(infer_param_value("3.14"), ParameterValue::from_double(3.14));
    }

    #[test]
    fn infer_param_value_string() {
        assert_eq!(
            infer_param_value("hello"),
            ParameterValue::from_string("hello").unwrap()
        );
    }
}
