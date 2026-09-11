//! Phase 269 (W0) — executor-shim: parameter-service FFI over the CppContext handle.
//!
//! Mirrors `nros-c/src/parameter.rs`'s executor-backed functions but recovers the
//! executor from `CppContext*` instead of `nros_executor_t*`. W1 emitters call these;
//! no emitter wires them yet this wave.
//!
//! Issue 0436 — user-supplied executor handles are tag-validated via
//! `cpp_ctx_checked` instead of being blind-cast to `*mut CppContext`.

// phase-426 W4 — the node-scoped entry points below are defined whatever the
// feature set is (see the block comment above them), so their signature
// vocabulary has to be too. Only the pieces that touch the STORE stay gated.
use core::ffi::c_char;

use crate::nros_cpp_ret_t;

#[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
use crate::NROS_CPP_RET_UNSUPPORTED;

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use core::ffi::c_void;

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use nros_node::ParameterValue;

#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
use crate::{
    NROS_CPP_RET_ALREADY_EXISTS, NROS_CPP_RET_ERROR, NROS_CPP_RET_FULL,
    NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_NOT_ALLOWED, NROS_CPP_RET_NOT_FOUND,
    NROS_CPP_RET_OK, cpp_ctx_checked, cstr_to_str,
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
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    match ctx.executor.register_parameter_services() {
        Ok(()) => NROS_CPP_RET_OK,
        Err(_) => NROS_CPP_RET_ERROR,
    }
}

/// Seed a launch parameter, given as a string, on the node the executor will
/// build NEXT.
///
/// issue 1272 -- the generated entry calls this for each launch `<param>` of a
/// node BEFORE it constructs that node (issue 0745: an rclcpp-shape
/// constructor reads its `declare_parameter` initials immediately), so the
/// node has no handle yet. `node` is the index the executor's `node_builder`
/// will give it: its position among the nodes its setup function builds on
/// this executor. An index that is not the next one is REFUSED with
/// `NROS_CPP_RET_INVALID_ARGUMENT` and a log line naming the parameter and
/// both indices. An earlier component that built two nodes, or none, would
/// otherwise move every later node's values onto a neighbour. Before 1272 the
/// call carried no node and every seed landed on the executor's primary node.
///
/// The value's type is still INFERRED from the string (bool, integer, double,
/// then string: `infer_param_value`, the same order `nros::main!` uses).
/// phase-446 carries the declared type from the launch contract instead; the
/// shim's one `infer_param_value` call is the place that changes.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` and `value` must be
/// valid null-terminated UTF-8 strings.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_declare_param(
    executor: *mut c_void,
    node: u8,
    name: *const c_char,
    value: *const c_char,
) -> nros_cpp_ret_t {
    if name.is_null() || value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    let val_str = match unsafe { cstr_to_str(value) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    // The seed predicts a NodeId that does not exist yet; it is right only if
    // every node built before this one took exactly one table row.
    let next = ctx.executor.nodes().len();
    if usize::from(node) != next {
        nros_log::log_error!(
            nros_log::get_logger("nros_cpp"),
            "launch parameter '{name_str}' names node index {node}, but the next node \
             this executor builds is index {next}: refusing to seed it on another node"
        );
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    // phase-446 seam: a declared type replaces this inference.
    let pv = infer_param_value(val_str);
    if ctx
        .executor
        .declare_parameter_on(nros_node::executor::NodeId::from_raw(node), name_str, pv)
    {
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
    if out_value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
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
    if out_value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
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

/// Get a boolean parameter by name from the C++ executor's parameter store.
///
/// Issue 0745 follow-up — the missing bool getter: without it, seeded bool
/// launch params were not ctor-adoptable (the C++ facades' launch-seed helpers
/// fell through to the compiled default for `bool`). Those helpers are gone
/// since phase-426 W4 — with one store there is nothing to copy across — but
/// this getter stays: it is the executor-scoped read the C API and the
/// generated entry still use.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. `name` must be valid null-terminated
/// UTF-8. `out_value` must be valid and writable.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_get_param_bool(
    executor: *mut c_void,
    name: *const c_char,
    out_value: *mut bool,
) -> nros_cpp_ret_t {
    if out_value.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let name_str = match unsafe { cstr_to_str(name) } {
        Some(s) => s,
        None => return NROS_CPP_RET_INVALID_ARGUMENT,
    };
    match ctx
        .executor
        .get_parameter(name_str)
        .and_then(|v| v.as_bool())
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
    if out_buf.is_null() || buf_len == 0 {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
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

// ============================================================================
// phase-426 W4 — the NODE-scoped parameter FFI
// ============================================================================
//
// Three stores existed for one concept: the executor's `nros_params` table
// (which the six `rcl_interfaces/srv/*` servers read), `rclcpp::Node`'s inline
// `ParameterServer<NROS_RCLCPP_MAX_PARAMS>`, and `nros::ComponentNode`'s own.
// A parameter declared through either C++ facade was invisible to `ros2 param
// get`, because the services read the first one and the facades wrote the other
// two. That is not a missing feature; it is a second implementation of one,
// which RFC-0019/0020 forbids. W4 deletes the two C++ members and points both
// facades HERE.
//
// WHY THESE TAKE A NODE HANDLE and the older `nros_cpp_*_param_*` above take a
// bare executor: phase-426 W1 keyed the store by NODE, because upstream's
// parameters belong to a node and an image composes several nodes onto one
// executor (RFC-0047). `nros_cpp_node_t` already carries the identity
// (`node_id`, biased by one since issue 0312), so the handle a C++ node already
// holds is the whole key. The executor-scoped functions stay: the GENERATED
// ENTRY seeds launch parameters through `nros_cpp_declare_param` before each
// component constructor runs, i.e. before that node has a table row, so it has
// no handle to pass.
//
// WHAT THAT MEANS FOR LAUNCH SEEDS (issue 1272): the seed names its node by
// the INDEX the executor will give it, and the shim refuses an index that is
// not the next one. The node's constructor then finds the seeded name already
// present under its own key and adopts it (issue 0745's contract), so two
// nodes that set the same name each read their own value. Before 1272 the
// seed carried no node and all of them landed on `NodeId::PRIMARY`: node 0
// adopted every node's values and a second node's identical name was refused.
//
// THEY ARE DEFINED UNCONDITIONALLY, unlike their executor-scoped neighbours,
// and that is a link-time decision. The C++ facades are on `rclcpp::Node` and
// `nros::ComponentNode`, which every C++ image compiles whether or not its
// bringup declares `param_services`; gating the methods on
// `NROS_SYSTEM_PARAM_SERVICES` would make a ported rclcpp node fail to compile
// on an image that never asked for services, and gating the SYMBOLS would make
// it fail to link. So the entry points always exist and answer
// `NROS_CPP_RET_UNSUPPORTED` when the store was not compiled in — a code the
// facades surface (`ComponentNode` makes it boot-fatal through `set_error`),
// never a silent default.

/// Resolve a C++ node handle to its executor context and its executor `NodeId`.
///
/// `None` for a null handle, a handle whose executor pointer is not one of ours
/// (the issue 0436 tag check), or a handle carrying no registered node — a
/// zero-initialised `rclcpp::Node` that was never opened. Guessing
/// `NodeId::PRIMARY` for the last case would write another node's parameters.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
unsafe fn node_param_target<'a>(
    node: *const crate::nros_cpp_node_t,
) -> Option<(&'a mut crate::CppContext, nros_node::executor::NodeId)> {
    if node.is_null() {
        return None;
    }
    let node = unsafe { &*node };
    let ctx = unsafe { cpp_ctx_checked(node.executor) }?;
    let id = crate::node_id_opt(node)?;
    Some((ctx, id))
}

/// Declare `value` on `node`, reporting whether the name was already there.
///
/// `NROS_CPP_RET_ALREADY_EXISTS` is not a failure at the facade: rclcpp's
/// contract is that `declare_parameter` ADOPTS an existing value (a launch
/// seed) rather than rejecting it, so the C++ side reads back on that code.
/// `declare` alone cannot say why it refused — it returns a bare `bool` for
/// both "taken" and "full" (`rust:ParameterServer::declare` has the gap
/// ledgered) — so the distinction is made here, once, instead of in each
/// facade.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
fn declare_on_node(
    ctx: &mut crate::CppContext,
    id: nros_node::executor::NodeId,
    name: &str,
    value: ParameterValue,
) -> nros_cpp_ret_t {
    if ctx.executor.declare_parameter_on(id, name, value) {
        NROS_CPP_RET_OK
    } else if ctx.executor.get_parameter_on(id, name).is_some() {
        NROS_CPP_RET_ALREADY_EXISTS
    } else {
        NROS_CPP_RET_FULL
    }
}

/// Map a wire-facing set verdict onto the C++ FFI codes.
///
/// The verdicts come from [`ParameterServer::apply`], which is the ONE place
/// that decides whether a set may happen — read-only, type, range, and issue
/// 1151's refusal of an undeclared name. Mapping them here rather than
/// collapsing them to `ERROR` is what lets a C++ caller tell "you cannot write
/// that parameter" from "that parameter does not exist".
///
/// [`ParameterServer::apply`]: nros_node::ParameterServer::apply
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
fn set_result_to_ret(result: nros_node::SetParameterResult) -> nros_cpp_ret_t {
    use nros_node::SetParameterResult as R;
    match result {
        R::Success => NROS_CPP_RET_OK,
        R::NotFound | R::Undeclared => NROS_CPP_RET_NOT_FOUND,
        R::ReadOnly => NROS_CPP_RET_NOT_ALLOWED,
        R::TypeMismatch | R::OutOfRange | R::InvalidRange => NROS_CPP_RET_INVALID_ARGUMENT,
        R::StorageFull => NROS_CPP_RET_FULL,
    }
}

/// Every node-scoped entry point below opens with the same four steps —
/// resolve the handle, resolve the name, run the body, or answer
/// `INVALID_ARGUMENT` / `UNSUPPORTED`. Writing them out nineteen times is how
/// one of them ends up checking three of the four; writing them as a macro
/// would hide the `extern "C"` signatures from cbindgen, which cannot expand
/// macros (the same reason `nros-c`'s `paste!`-generated array FFI has to be
/// re-declared by hand in `parameter.hpp`). So the signature stays literal and
/// only the PROLOGUE is shared.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
macro_rules! node_param_prologue {
    ($node:expr, $name:expr) => {{
        let Some((ctx, id)) = (unsafe { node_param_target($node) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(name) = (unsafe { cstr_to_str($name) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        (ctx, id, name)
    }};
}

/// Declare a `bool` parameter on this node, in the executor's store.
///
/// # Safety
/// `node` must be null or point to an `nros_cpp_node_t` opened by
/// `nros_cpp_node_create*`. `name` must be a valid null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_bool(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: bool,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        declare_on_node(ctx, id, name, ParameterValue::from_bool(value))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Declare an integer parameter on this node. See
/// [`nros_cpp_node_declare_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_integer(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: i64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        declare_on_node(ctx, id, name, ParameterValue::from_integer(value))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Declare a double parameter on this node. See
/// [`nros_cpp_node_declare_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_double(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: f64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        declare_on_node(ctx, id, name, ParameterValue::from_double(value))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Declare a string parameter on this node. A value longer than the store's
/// `MAX_STRING_VALUE_LEN` is REFUSED (`NROS_CPP_RET_FULL`), never truncated —
/// a silently shortened frame id is a wrong value, not a smaller one.
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`]; `value` must also be a valid
/// null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_string(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: *const c_char,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(value) = (unsafe { cstr_to_str(value) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_string(value) else {
            return NROS_CPP_RET_FULL;
        };
        declare_on_node(ctx, id, name, pv)
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Read a `bool` parameter of this node.
///
/// `NROS_CPP_RET_NOT_FOUND` when the name is undeclared for this node OR holds
/// another type — the C++ facade's `get_parameter<T>` asks a typed question and
/// a wrong-typed answer is not one.
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`]; `out_value` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_bool(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out_value: *mut bool,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_value.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        match ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_bool())
        {
            Some(v) => {
                unsafe { *out_value = v };
                NROS_CPP_RET_OK
            }
            None => NROS_CPP_RET_NOT_FOUND,
        }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out_value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Read an integer parameter of this node. See
/// [`nros_cpp_node_get_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_get_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_integer(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out_value: *mut i64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_value.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        match ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_integer())
        {
            Some(v) => {
                unsafe { *out_value = v };
                NROS_CPP_RET_OK
            }
            None => NROS_CPP_RET_NOT_FOUND,
        }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out_value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Read a double parameter of this node. See
/// [`nros_cpp_node_get_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_get_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_double(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out_value: *mut f64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_value.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        match ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_double())
        {
            Some(v) => {
                unsafe { *out_value = v };
                NROS_CPP_RET_OK
            }
            None => NROS_CPP_RET_NOT_FOUND,
        }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out_value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Read a string parameter of this node into `out_buf`, null-terminated.
///
/// `NROS_CPP_RET_FULL` when the value did not fit: the buffer holds the
/// truncated prefix, so a caller that ignores the code still holds a valid
/// C string.
///
/// # Safety
/// As [`nros_cpp_node_get_param_bool`]; `out_buf` must be valid for `buf_len`
/// bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_string(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out_buf: *mut c_char,
    buf_len: usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_buf.is_null() || buf_len == 0 {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(val) = ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_string())
        else {
            return NROS_CPP_RET_NOT_FOUND;
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
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out_buf, buf_len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Set an already-declared `bool` parameter of this node.
///
/// Routes through `Executor::set_parameter_on`, i.e. through
/// `ParameterServer::apply` — the same read-only / type / range rules a remote
/// `ros2 param set` gets. A C++ facade that wrote the slot directly would be a
/// second answer to "may this set happen".
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_set_param_bool(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: bool,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        set_result_to_ret(
            ctx.executor
                .set_parameter_on(id, name, ParameterValue::from_bool(value)),
        )
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Set an already-declared integer parameter of this node. See
/// [`nros_cpp_node_set_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_set_param_integer(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: i64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        set_result_to_ret(ctx.executor.set_parameter_on(
            id,
            name,
            ParameterValue::from_integer(value),
        ))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Set an already-declared double parameter of this node. See
/// [`nros_cpp_node_set_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_set_param_double(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: f64,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        set_result_to_ret(ctx.executor.set_parameter_on(
            id,
            name,
            ParameterValue::from_double(value),
        ))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Set an already-declared string parameter of this node. See
/// [`nros_cpp_node_set_param_bool`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_string`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_set_param_string(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    value: *const c_char,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(value) = (unsafe { cstr_to_str(value) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_string(value) else {
            return NROS_CPP_RET_FULL;
        };
        set_result_to_ret(ctx.executor.set_parameter_on(id, name, pv))
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, value);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Is `name` declared for this node?
///
/// A `bool`, not a `nros_cpp_ret_t`: `has_parameter` is upstream's spelling and
/// answers a yes/no question. "No store compiled in" is therefore
/// indistinguishable from "not declared", which is the right collapse — in both
/// cases the node does not have that parameter.
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_has_param(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
) -> bool {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let Some((ctx, id)) = (unsafe { node_param_target(node) }) else {
            return false;
        };
        let Some(name) = (unsafe { cstr_to_str(name) }) else {
            return false;
        };
        ctx.executor.get_parameter_on(id, name).is_some()
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name);
        false
    }
}

// --- array parameters -------------------------------------------------------
//
// `nros::ComponentNode::declare_parameter<std::vector<T>>` (hosted) is the only
// caller, and it is why these exist at all: deleting the C++ store without them
// would have turned a working `std::vector<double>` weight matrix into a
// silently-defaulted one. Three element types, matching the store's own
// (`BoolArray` / `IntegerArray` / `DoubleArray`); a string array has no C++
// facade to serve.
//
// Values are COPIED into the store, unlike `nros-c`'s `nros_parameter_*_array`,
// which records a borrowed pointer the caller must keep alive. That difference
// is the whole reason the C++ store needed its own `seq_pool_` — with the Rust
// store owning the elements, the pool goes too.

/// Declare a double-array parameter on this node, copying `len` elements.
///
/// `NROS_CPP_RET_FULL` when `len` exceeds the store's `MAX_ARRAY_LEN`.
///
/// # Safety
/// As [`nros_cpp_node_declare_param_bool`]; `data` must be valid for `len`
/// elements (it may be null when `len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_double_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    data: *const f64,
    len: usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(slice) = (unsafe { slice_or_empty(data, len) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_double_array(slice) else {
            return NROS_CPP_RET_FULL;
        };
        declare_on_node(ctx, id, name, pv)
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, data, len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Declare an integer-array parameter on this node. See
/// [`nros_cpp_node_declare_param_double_array`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_double_array`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_integer_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    data: *const i64,
    len: usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(slice) = (unsafe { slice_or_empty(data, len) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_integer_array(slice) else {
            return NROS_CPP_RET_FULL;
        };
        declare_on_node(ctx, id, name, pv)
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, data, len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Declare a bool-array parameter on this node. See
/// [`nros_cpp_node_declare_param_double_array`].
///
/// # Safety
/// As [`nros_cpp_node_declare_param_double_array`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_declare_param_bool_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    data: *const bool,
    len: usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(slice) = (unsafe { slice_or_empty(data, len) }) else {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        };
        let Some(pv) = ParameterValue::from_bool_array(slice) else {
            return NROS_CPP_RET_FULL;
        };
        declare_on_node(ctx, id, name, pv)
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, data, len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Copy a double-array parameter of this node into `out`, writing the element
/// count to `out_len`.
///
/// `NROS_CPP_RET_FULL` when the stored array is longer than `capacity` —
/// NOTHING is written in that case and `*out_len` carries the length the caller
/// would have needed. Truncating a weight matrix produces a plausible wrong
/// answer, which is worse than none.
///
/// # Safety
/// As [`nros_cpp_node_get_param_bool`]; `out` must be valid for `capacity`
/// elements and `out_len` must be writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_double_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out: *mut f64,
    capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_len.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(src) = ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_double_array())
        else {
            return NROS_CPP_RET_NOT_FOUND;
        };
        unsafe { copy_array_out(src, out, capacity, out_len) }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out, capacity, out_len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Copy an integer-array parameter of this node. See
/// [`nros_cpp_node_get_param_double_array`].
///
/// # Safety
/// As [`nros_cpp_node_get_param_double_array`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_integer_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out: *mut i64,
    capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_len.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(src) = ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_integer_array())
        else {
            return NROS_CPP_RET_NOT_FOUND;
        };
        unsafe { copy_array_out(src, out, capacity, out_len) }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out, capacity, out_len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Copy a bool-array parameter of this node. See
/// [`nros_cpp_node_get_param_double_array`].
///
/// # Safety
/// As [`nros_cpp_node_get_param_double_array`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_node_get_param_bool_array(
    node: *const crate::nros_cpp_node_t,
    name: *const c_char,
    out: *mut bool,
    capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    #[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
    {
        if out_len.is_null() {
            return NROS_CPP_RET_INVALID_ARGUMENT;
        }
        let (ctx, id, name) = node_param_prologue!(node, name);
        let Some(src) = ctx
            .executor
            .get_parameter_on(id, name)
            .and_then(|v| v.as_bool_array())
        else {
            return NROS_CPP_RET_NOT_FOUND;
        };
        unsafe { copy_array_out(src, out, capacity, out_len) }
    }
    #[cfg(not(all(feature = "param-services", feature = "rmw-cffi")))]
    {
        let _ = (node, name, out, capacity, out_len);
        NROS_CPP_RET_UNSUPPORTED
    }
}

/// Borrow `len` elements from `data`, tolerating a null pointer at `len == 0`.
///
/// An empty `std::vector` yields `data() == nullptr` on some implementations,
/// and `core::slice::from_raw_parts` requires a non-null, aligned pointer even
/// for a zero length — so the empty case cannot go through it.
///
/// # Safety
/// `data` must be valid for `len` elements when `len != 0`.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
unsafe fn slice_or_empty<'a, T>(data: *const T, len: usize) -> Option<&'a [T]> {
    if len == 0 {
        return Some(&[]);
    }
    if data.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(data, len) })
}

/// Copy `src` into `out`, or refuse if it does not fit. `*out_len` always
/// receives `src.len()`, so a `FULL` caller can size a second attempt.
///
/// # Safety
/// `out` must be valid for `capacity` elements; `out_len` must be writable.
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]
unsafe fn copy_array_out<T: Copy>(
    src: &[T],
    out: *mut T,
    capacity: usize,
    out_len: *mut usize,
) -> nros_cpp_ret_t {
    unsafe { *out_len = src.len() };
    if src.len() > capacity {
        return NROS_CPP_RET_FULL;
    }
    if src.is_empty() {
        return NROS_CPP_RET_OK;
    }
    if out.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), out, src.len()) };
    NROS_CPP_RET_OK
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

    /// `ParameterValue` has no `PartialEq` (`nros-params/src/types.rs:127`
    /// derives `Debug, Clone, Default` and nothing else), so the assertions in
    /// this module were written as `assert_eq!` and NEVER COMPILED — every one
    /// of them is `binary operation == cannot be applied`.
    ///
    /// They went unnoticed because no lane builds them: the whole module is
    /// behind `param-services + rmw-cffi`, and `just test-unit` runs
    /// `cargo nextest --workspace` with NO features, so `nros-cpp`'s lib test
    /// target is compiled with the cfg OFF. A test nothing compiles is the
    /// `check-no-vacuous-tests` class one level down — it reads as coverage and
    /// asserts nothing — so phase-426 W4 fixed the assertions rather than
    /// leaving them beside new ones that do run.
    ///
    /// Comparing through this helper rather than adding a `PartialEq` derive:
    /// the derive would be public API on a type with a wildcard ledger row, and
    /// a test's convenience is not a reason to widen a surface.
    fn same_value(a: &ParameterValue, b: &ParameterValue) -> bool {
        match (a, b) {
            (ParameterValue::NotSet, ParameterValue::NotSet) => true,
            (ParameterValue::Bool(x), ParameterValue::Bool(y)) => x == y,
            (ParameterValue::Integer(x), ParameterValue::Integer(y)) => x == y,
            (ParameterValue::Double(x), ParameterValue::Double(y)) => x == y,
            (ParameterValue::String(x), ParameterValue::String(y)) => x == y,
            (ParameterValue::BoolArray(x), ParameterValue::BoolArray(y)) => x == y,
            (ParameterValue::IntegerArray(x), ParameterValue::IntegerArray(y)) => x == y,
            (ParameterValue::DoubleArray(x), ParameterValue::DoubleArray(y)) => x == y,
            _ => false,
        }
    }

    macro_rules! assert_value_eq {
        ($a:expr, $b:expr) => {{
            let a = $a;
            let b = $b;
            assert!(same_value(&a, &b), "{:?} != {:?}", a, b);
        }};
    }

    /// Null-pointer guard: every shim fn returns INVALID_ARGUMENT for a null executor.
    #[test]
    fn null_executor_returns_invalid_argument() {
        let ret = unsafe { nros_cpp_register_parameter_services(ptr::null_mut()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let name = c"p";
        let val = c"v";
        let ret =
            unsafe { nros_cpp_declare_param(ptr::null_mut(), 0, name.as_ptr(), val.as_ptr()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
    }

    #[test]
    fn infer_param_value_bool() {
        assert_value_eq!(infer_param_value("true"), ParameterValue::from_bool(true));
        assert_value_eq!(infer_param_value("false"), ParameterValue::from_bool(false));
        assert_value_eq!(infer_param_value("True"), ParameterValue::from_bool(true));
        assert_value_eq!(infer_param_value("FALSE"), ParameterValue::from_bool(false));
    }

    #[test]
    fn infer_param_value_integer() {
        assert_value_eq!(infer_param_value("42"), ParameterValue::from_integer(42));
        assert_value_eq!(infer_param_value("-7"), ParameterValue::from_integer(-7));
    }

    #[test]
    fn infer_param_value_double() {
        assert_value_eq!(infer_param_value("3.14"), ParameterValue::from_double(3.14));
    }

    #[test]
    fn infer_param_value_string() {
        assert_value_eq!(
            infer_param_value("hello"),
            ParameterValue::from_string("hello").unwrap()
        );
    }

    // --- phase-426 W4 -------------------------------------------------------

    /// A node handle that is null, or whose executor pointer is not one of ours,
    /// must never reach a store. INVALID_ARGUMENT rather than a default: writing
    /// through an untagged pointer is issue 0436's memory corruption, and
    /// GUESSING `NodeId::PRIMARY` for a handle carrying no node would write some
    /// other node's parameters.
    #[test]
    fn a_bad_node_handle_reaches_no_store() {
        let name = c"rate";
        let value = c"5";

        assert_eq!(
            unsafe { nros_cpp_node_declare_param_double(ptr::null(), name.as_ptr(), 1.0) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe { nros_cpp_node_set_param_integer(ptr::null(), name.as_ptr(), 1) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        let mut out = 0i64;
        assert_eq!(
            unsafe { nros_cpp_node_get_param_integer(ptr::null(), name.as_ptr(), &mut out) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(out, 0, "a refused read must not write the out-parameter");
        assert!(!unsafe { nros_cpp_node_has_param(ptr::null(), name.as_ptr()) });

        // A zero-initialised handle: non-null, but its executor pointer is null
        // and its `node_id` field is the issue-0312 "no node registered"
        // sentinel. Both halves must refuse.
        let handle = zeroed_node_handle();
        assert_eq!(
            unsafe { nros_cpp_node_declare_param_string(&handle, name.as_ptr(), value.as_ptr()) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert!(!unsafe { nros_cpp_node_has_param(&handle, name.as_ptr()) });
    }

    /// The out-pointer guards fire BEFORE the handle is dereferenced, so a null
    /// buffer is INVALID_ARGUMENT and not a wild write.
    #[test]
    fn null_out_parameters_are_refused() {
        let name = c"rate";
        assert_eq!(
            unsafe { nros_cpp_node_get_param_double(ptr::null(), name.as_ptr(), ptr::null_mut()) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            unsafe {
                nros_cpp_node_get_param_string(ptr::null(), name.as_ptr(), ptr::null_mut(), 8)
            },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        // A non-null buffer of length ZERO cannot hold even the terminator.
        let mut byte = 0 as c_char;
        assert_eq!(
            unsafe { nros_cpp_node_get_param_string(ptr::null(), name.as_ptr(), &mut byte, 0) },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        let mut len = 0usize;
        assert_eq!(
            unsafe {
                nros_cpp_node_get_param_double_array(
                    ptr::null(),
                    name.as_ptr(),
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                )
            },
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        let _ = &mut len;
    }

    /// Every wire-facing verdict maps to a DISTINCT C++ code where the C++ side
    /// can act on the difference, and the codes are pinned by value because the
    /// facades compare against `NROS_RET_ALREADY_EXISTS` and
    /// `ErrorCode::Unsupported` by number (issue #229's one numbering).
    #[test]
    fn set_verdicts_map_to_actionable_codes() {
        use nros_node::SetParameterResult as R;
        assert_eq!(set_result_to_ret(R::Success), NROS_CPP_RET_OK);
        // "you cannot write that" is not "it is not there": a read-only
        // parameter EXISTS, and a facade that reported NOT_FOUND for it would
        // send a porting user looking for a missing declaration.
        assert_eq!(set_result_to_ret(R::ReadOnly), NROS_CPP_RET_NOT_ALLOWED);
        assert_eq!(set_result_to_ret(R::NotFound), NROS_CPP_RET_NOT_FOUND);
        assert_eq!(set_result_to_ret(R::Undeclared), NROS_CPP_RET_NOT_FOUND);
        assert_eq!(
            set_result_to_ret(R::TypeMismatch),
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            set_result_to_ret(R::OutOfRange),
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(
            set_result_to_ret(R::InvalidRange),
            NROS_CPP_RET_INVALID_ARGUMENT
        );
        assert_eq!(set_result_to_ret(R::StorageFull), NROS_CPP_RET_FULL);
    }

    /// An empty `std::vector` may hand us `nullptr` with length 0, which
    /// `slice::from_raw_parts` will not accept — that case has to be answered
    /// without ever forming the slice.
    #[test]
    fn an_empty_array_may_arrive_as_a_null_pointer() {
        assert_eq!(
            unsafe { slice_or_empty(ptr::null::<f64>(), 0) },
            Some(&[][..])
        );
        // A null pointer with a NON-zero length is a caller bug, not an empty
        // array, and must be refused rather than read.
        assert!(unsafe { slice_or_empty(ptr::null::<f64>(), 3) }.is_none());
        let src = [1.0f64, 2.0, 3.0];
        assert_eq!(
            unsafe { slice_or_empty(src.as_ptr(), 3) },
            Some(&src[..]),
            "a real pointer must borrow exactly `len` elements"
        );
    }

    /// A too-small destination is REFUSED with the required length, never
    /// truncated. A truncated weight matrix is a plausible wrong answer; a
    /// refusal the caller can resize from is not.
    #[test]
    fn an_oversized_array_is_refused_with_its_length() {
        let src = [1.0f64, 2.0, 3.0];
        let mut dst = [0.0f64; 2];
        let mut len = 0usize;
        let ret = unsafe { copy_array_out(&src, dst.as_mut_ptr(), dst.len(), &mut len) };
        assert_eq!(ret, NROS_CPP_RET_FULL);
        assert_eq!(len, 3, "the caller must learn the size to retry with");
        assert_eq!(dst, [0.0, 0.0], "nothing may be written on refusal");

        let mut big = [0.0f64; 4];
        let ret = unsafe { copy_array_out(&src, big.as_mut_ptr(), big.len(), &mut len) };
        assert_eq!(ret, NROS_CPP_RET_OK);
        assert_eq!(len, 3);
        assert_eq!(big, [1.0, 2.0, 3.0, 0.0]);

        // The capacity PROBE the C++ `std::vector` reader opens with: capacity 0
        // and a null destination, which must answer the length and not write.
        let ret = unsafe { copy_array_out(&src, ptr::null_mut::<f64>(), 0, &mut len) };
        assert_eq!(ret, NROS_CPP_RET_FULL);
        assert_eq!(len, 3);

        // An EMPTY stored array through the same probe is OK, not FULL, so the
        // reader stops after one call.
        let ret = unsafe { copy_array_out(&[] as &[f64], ptr::null_mut::<f64>(), 0, &mut len) };
        assert_eq!(ret, NROS_CPP_RET_OK);
        assert_eq!(len, 0);
    }

    /// The STORE's capacity is what bounds an array value, so a slice past it
    /// is refused rather than silently shortened — the C++ `std::vector` facade
    /// turns that into `ErrorCode::Full` and a boot-fatal `set_error`, not a
    /// half-copied weight matrix.
    ///
    /// The bound is ASKED FOR rather than named: `MAX_ARRAY_LEN` is a build
    /// knob (`NROS_MAX_ARRAY_LEN`) and `nros-node` does not re-export it, so a
    /// test that spelled a number would be asserting this build's knob rather
    /// than the property.
    #[test]
    fn an_array_longer_than_the_store_is_refused() {
        let buf = [0.0f64; 4096];
        let mut cap = 0usize;
        for n in 1..=buf.len() {
            if ParameterValue::from_double_array(&buf[..n]).is_some() {
                cap = n;
            } else {
                break;
            }
        }
        assert!(cap >= 1, "the store accepts no array at all");
        assert!(cap < buf.len(), "the probe never found a refusal");
        assert!(ParameterValue::from_double_array(&buf[..cap]).is_some());
        assert!(ParameterValue::from_double_array(&buf[..cap + 1]).is_none());
        // The other two element types share the bound.
        let ints = [0i64; 4096];
        assert!(ParameterValue::from_integer_array(&ints[..cap]).is_some());
        assert!(ParameterValue::from_integer_array(&ints[..cap + 1]).is_none());
        let bools = [false; 4096];
        assert!(ParameterValue::from_bool_array(&bools[..cap]).is_some());
        assert!(ParameterValue::from_bool_array(&bools[..cap + 1]).is_none());
    }

    /// A zero-initialised `nros_cpp_node_t`, the shape a C++ `rclcpp::Node` has
    /// before `nros_cpp_node_create` runs.
    fn zeroed_node_handle() -> crate::nros_cpp_node_t {
        crate::nros_cpp_node_t {
            executor: ptr::null_mut(),
            name: [0u8; crate::NROS_CPP_NAME_LEN],
            namespace: [0u8; crate::NROS_CPP_NAMESPACE_LEN],
            node_id: 0,
            _reserved: [0u8; crate::NROS_CPP_NODE_RESERVED],
            qos_overrides: ptr::null(),
            qos_overrides_len: 0,
        }
    }
}
