//! Phase 269 (W0) — executor-shim: lifecycle FFI over the CppContext handle.
//!
//! Mirrors `nros-c/src/lifecycle.rs`'s service-backed module but recovers the
//! executor from `CppContext*` instead of `rclc_executor_t*`. W1/W2 emitters
//! call these; no emitter wires them yet this wave.
//!
//! Issue 0436 — user-supplied executor handles are tag-validated via
//! `cpp_ctx_checked` instead of being blind-cast to `*mut CppContext`.

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use core::ffi::c_void;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use nros_node::LifecycleTransition;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use nros_node::lifecycle::{LifecycleCallbackSlot, LifecycleError};

/// C callback type for a lifecycle transition: `uint8_t callback(void* context)`.
///
/// The return value is a REP-2002 `TransitionResult` (`Success = 0`, `Failure = 1`,
/// `Error = 2`). Registered per transition via `nros_cpp_lifecycle_register_on_*`;
/// all six callbacks on one node share the single `ctx` set by the last register call
/// (the C++ `LifecycleNode` wrapper always passes `this`).
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
pub type nros_cpp_lifecycle_callback_t = unsafe extern "C" fn(*mut c_void) -> u8;

#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
use crate::{
    NROS_CPP_RET_ERROR, NROS_CPP_RET_INVALID_ARGUMENT, NROS_CPP_RET_NOT_FOUND,
    NROS_CPP_RET_NOT_INIT, NROS_CPP_RET_OK, cpp_ctx_checked, nros_cpp_ret_t,
};

/// Register the five REP-2002 lifecycle services on the C++ executor's node.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_register_lifecycle_services(
    executor: *mut c_void,
) -> nros_cpp_ret_t {
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    match ctx.executor.register_lifecycle_services() {
        Ok(()) => NROS_CPP_RET_OK,
        // Issue 2304 -- say why, not a bare ERROR.
        Err(e) => crate::report_entity_refusal(
            "nros_cpp_register_lifecycle_services",
            format_args!("the lifecycle services"),
            e,
        ),
    }
}

/// Register the five REP-2002 lifecycle services under `node`'s own name.
///
/// phase-488 node 2 — what `rclcpp_lifecycle::LifecycleNode` calls once it is
/// bound to its node. [`nros_cpp_register_lifecycle_services`] names the
/// services after the EXECUTOR's identity, which for the process executor a
/// hosted node is created on is the default `node`: `ros2 lifecycle get
/// /<name>` then answered "Node not found".
///
/// # Safety
/// `node` must be a live `nros_cpp_node_t*` whose executor is a valid
/// `CppContext*`.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_register_lifecycle_services_for_node(
    node: *const crate::nros_cpp_node_t,
) -> nros_cpp_ret_t {
    if node.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let node = unsafe { &*node };
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(node.executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(id) = crate::node_id_opt(node) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    match ctx.executor.register_lifecycle_services_on(id) {
        Ok(()) => NROS_CPP_RET_OK,
        // Issue 2304 -- say why, not a bare ERROR.
        Err(e) => crate::report_entity_refusal(
            "nros_cpp_register_lifecycle_services_on",
            format_args!("the lifecycle services"),
            e,
        ),
    }
}

/// Trigger a lifecycle transition on the C++ executor's state machine.
///
/// `transition_id` is a `lifecycle_msgs/msg/Transition` id — the SAME numbering
/// rclcpp uses (issue 1099): Configure=1, Cleanup=2, Activate=3, Deactivate=4,
/// UnconfiguredShutdown=5, InactiveShutdown=6, ActiveShutdown=7,
/// ErrorRecovery=60. `0` (CREATE) and `8` (DESTROY) are not implemented and
/// return `INVALID_ARGUMENT`.
///
/// (This comment used to read "Configure=1, Activate=2, Deactivate=3,
/// Cleanup=4, Shutdown=5, ErrorProcessed=6" — three of those were nano-ros's
/// own numbering and ~~`ErrorProcessed=6`~~ was never a transition at all.)
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`. Any registered transition
/// callbacks are invoked through raw function pointers; the caller must ensure
/// they and their captured context remain live.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_change_state(
    executor: *mut c_void,
    // `transition_id` is a `lifecycle_msgs/msg/Transition` id (issue 1099) —
    // the same number rclcpp means. Unimplemented ids (`0` CREATE, `8`
    // DESTROY) fall out of `from_u8` as `INVALID_ARGUMENT`.
    transition_id: u8,
) -> nros_cpp_ret_t {
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(sm) = ctx.executor.lifecycle_state_machine_mut() else {
        return NROS_CPP_RET_NOT_INIT;
    };
    let Some(t) = LifecycleTransition::from_u8(transition_id) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    // SAFETY: forwarded through this function's unsafe contract.
    match unsafe { sm.trigger_transition(t) } {
        Ok(_) => NROS_CPP_RET_OK,
        Err(LifecycleError::NodeFinalized) => NROS_CPP_RET_ERROR,
        Err(LifecycleError::InvalidTransition { .. }) => NROS_CPP_RET_INVALID_ARGUMENT,
        Err(LifecycleError::CallbackFailed { .. }) => NROS_CPP_RET_ERROR,
    }
}

/// Get the current REP-2002 lifecycle state of the C++ executor's state machine.
///
/// Returns `0` if the executor is null or lifecycle services are not registered
/// yet — that is the `Unknown` sentinel (`rclcpp_lifecycle::LifecycleState::Unknown`), NOT
/// `Unconfigured`. State numbering is `lifecycle_msgs/msg/State`'s:
/// `Unconfigured = 1`, `Inactive = 2`, `Active = 3`, `Finalized = 4`, plus
/// `ErrorProcessing = 5` (upstream numbers that one 15; see
/// `nros_core::lifecycle`). This comment previously documented a 0-based
/// numbering that has never been what the function returns.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_get_current_state(executor: *mut c_void) -> u8 {
    let Some(ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return 0;
    };
    match ctx.executor.lifecycle_state_machine() {
        Some(sm) => sm.state() as u8,
        None => 0,
    }
}

/// Shared body for the six `nros_cpp_lifecycle_register_on_*` shims: recover the
/// `CppContext`'s executor state machine, install `cb` in `slot`, and point the
/// machine's single callback context at `ctx`.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*`; `cb` and `ctx` must remain live
/// for as long as the callback can fire.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
unsafe fn register_cpp(
    executor: *mut c_void,
    slot: LifecycleCallbackSlot,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    let Some(mut context) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(sm) = context.executor.lifecycle_state_machine_mut() else {
        return NROS_CPP_RET_NOT_INIT;
    };
    sm.register(slot, Some(cb));
    sm.set_context(ctx);
    NROS_CPP_RET_OK
}

/// Register the `on_configure` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_configure(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Configure, cb, ctx) }
}

/// Register the `on_activate` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_activate(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Activate, cb, ctx) }
}

/// Register the `on_deactivate` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_deactivate(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Deactivate, cb, ctx) }
}

/// Register the `on_cleanup` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_cleanup(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Cleanup, cb, ctx) }
}

/// Register the `on_shutdown` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_shutdown(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Shutdown, cb, ctx) }
}

/// Register the `on_error` transition callback on the C++ executor's state machine.
///
/// # Safety
/// See [`register_cpp`].
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_on_error(
    executor: *mut c_void,
    cb: nros_cpp_lifecycle_callback_t,
    ctx: *mut c_void,
) -> nros_cpp_ret_t {
    unsafe { register_cpp(executor, LifecycleCallbackSlot::Error, cb, ctx) }
}

/// Make one MANAGED node of the image a lifecycle node: register the five
/// REP-2002 services under the node's own name, then drive it to its boot
/// autostart.
///
/// The generated C and C++ entries call this right after building each node
/// the resolved model marks managed (a launch `<lifecycle_node>`), with the
/// node's own `autostart_code`: 0 = services only (none), 1 = configure,
/// 2 = configure + activate. No machine is registered on the executor's own
/// identity. A node whose component already registered its own machine (a
/// C++ `rclcpp_lifecycle::LifecycleNode`) keeps it, with its callbacks, and
/// each autostart step runs only from the state it starts from.
///
/// Returns `NROS_CPP_RET_NOT_FOUND` (and logs the node) when no node with
/// that name and namespace is in the executor's node table.
///
/// # Safety
/// `executor` must be a valid, live `CppContext*` produced by `nros_cpp_init`;
/// `namespace_` and `name` must be NUL-terminated strings (`namespace_` may be
/// null for the root). Registered transition callbacks run here, so they and
/// their context must be live.
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_cpp_lifecycle_register_node(
    executor: *mut c_void,
    namespace_: *const core::ffi::c_char,
    name: *const core::ffi::c_char,
    autostart_code: u8,
) -> nros_cpp_ret_t {
    if name.is_null() {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    }
    let Some(mut ctx) = (unsafe { cpp_ctx_checked(executor) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let Some(name) = (unsafe { crate::cstr_to_str(name) }) else {
        return NROS_CPP_RET_INVALID_ARGUMENT;
    };
    let ns = if namespace_.is_null() {
        "/"
    } else {
        match unsafe { crate::cstr_to_str(namespace_) } {
            Some(s) => s,
            None => return NROS_CPP_RET_INVALID_ARGUMENT,
        }
    };
    let Some(node) = ctx.executor.managed_node_id(name, ns) else {
        crate::cpp_diag!(
            "nros_cpp_lifecycle_register_node: the launch file marks node `{}` in namespace \
             `{}` managed, but no node by that name is in the executor's node table",
            name,
            ns
        );
        return NROS_CPP_RET_NOT_FOUND;
    };
    if let Err(e) = ctx.executor.register_managed_node_on(node) {
        // Issue 2304 -- say why, not a bare ERROR.
        return crate::report_entity_refusal(
            "nros_cpp_lifecycle_register_node",
            format_args!("the lifecycle services of `{}`", name),
            e,
        );
    }
    // SAFETY: forwarded through this function's unsafe contract.
    match unsafe {
        ctx.executor
            .drive_lifecycle_autostart_on(node, autostart_code)
    } {
        Ok(()) => NROS_CPP_RET_OK,
        Err(LifecycleError::InvalidTransition { .. }) => NROS_CPP_RET_INVALID_ARGUMENT,
        Err(LifecycleError::NodeFinalized) | Err(LifecycleError::CallbackFailed { .. }) => {
            NROS_CPP_RET_ERROR
        }
    }
}

#[cfg(test)]
#[cfg(all(feature = "lifecycle-services", feature = "rmw-cffi"))]
mod tests {
    use core::ptr;

    use super::*;

    unsafe extern "C" fn dummy_cb(_ctx: *mut core::ffi::c_void) -> u8 {
        0
    }

    /// Null-pointer guard: every shim fn returns INVALID_ARGUMENT for a null executor.
    #[test]
    fn null_executor_returns_invalid_argument() {
        let ret = unsafe { nros_cpp_register_lifecycle_services(ptr::null_mut()) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let ret = unsafe { nros_cpp_lifecycle_change_state(ptr::null_mut(), 1) };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        let ret = unsafe {
            nros_cpp_lifecycle_register_node(ptr::null_mut(), ptr::null(), c"talker".as_ptr(), 0)
        };
        assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        for reg in [
            nros_cpp_lifecycle_register_on_configure as unsafe extern "C" fn(_, _, _) -> _,
            nros_cpp_lifecycle_register_on_activate,
            nros_cpp_lifecycle_register_on_deactivate,
            nros_cpp_lifecycle_register_on_cleanup,
            nros_cpp_lifecycle_register_on_shutdown,
            nros_cpp_lifecycle_register_on_error,
        ] {
            let ret = unsafe { reg(ptr::null_mut(), dummy_cb, ptr::null_mut()) };
            assert_eq!(ret, NROS_CPP_RET_INVALID_ARGUMENT);
        }
    }

    /// Null executor reads back as `Unconfigured` (0) rather than trapping.
    #[test]
    fn null_executor_get_current_state_is_unconfigured() {
        assert_eq!(
            unsafe { nros_cpp_lifecycle_get_current_state(ptr::null_mut()) },
            0
        );
    }
}
