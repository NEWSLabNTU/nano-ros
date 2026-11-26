// Phase 579 W3a; RFC-0139 D4 and W1b.
//! The device extension hook for C: register a device event source on an
//! executor and get the completion handle its port signals.
//!
//! This is the extension surface, not the ROS user API: it is declared in
//! `<nros/device_hook.h>`, never under `rclc/`, and the companion package's
//! worker and source registration build on it. It is deliberately left out
//! of the generated `rclc` header.

use core::ffi::c_void;

use crate::error::*;

/// `nros_completion_t` from `<nros/device_port.h>`: one opaque pointer, by
/// value. Mirrored here rather than imported, since this crate does not depend
/// on the RMW ABI crate; the layout check below and the C one keep it honest.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct nros_completion_t {
    pub _slot: *mut c_void,
}

const _: () = assert!(
    core::mem::size_of::<nros_completion_t>() == core::mem::size_of::<*mut c_void>()
        && core::mem::align_of::<nros_completion_t>() == core::mem::align_of::<*mut c_void>()
);

/// The continuation a device runs when its completion is signalled, with the
/// context it was registered with.
pub type nros_device_callback_t = Option<unsafe extern "C" fn(context: *mut c_void)>;

/// Register a device event source on `executor`.
///
/// Claims one callback slot, whose handle is written to `out_handle` (for
/// `rclc_executor_bind_handle_to_sched_context`), and writes the completion
/// handle the device's port signals to `out_done`. The handle stays valid for
/// the executor's life. Register before the first spin.
///
/// # Safety
/// `executor` must point to an initialised executor; `out_handle` and
/// `out_done` to writable storage. `callback` and `context` must stay valid
/// for the executor's life.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nros_executor_register_device(
    executor: *mut crate::executor::rclc_executor_t,
    callback: nros_device_callback_t,
    context: *mut c_void,
    out_handle: *mut usize,
    out_done: *mut nros_completion_t,
) -> rcl_ret_t {
    validate_not_null!(executor, out_handle, out_done);
    let Some(cb) = callback else {
        return RCL_RET_INVALID_ARGUMENT;
    };
    let exec = unsafe { &mut *executor };
    validate_state!(
        exec,
        crate::executor::rclc_executor_state_t::RCLC_EXECUTOR_STATE_INITIALIZED
    );
    if exec.handle_count >= exec.max_handles {
        return crate::executor::handles_full(exec, "nros_executor_register_device");
    }
    let mut rust_exec = unsafe { crate::executor::get_executor(executor) };
    // SAFETY: the caller keeps the callback and its context alive for the
    // executor's life; registration is one-way.
    let wrapper = move || unsafe { cb(context) };
    match rust_exec.register_device(wrapper) {
        Ok((handle_id, done)) => {
            // A callback slot the recording backend never sees (issue 2984).
            nros::census_hooks::on_device_create();
            unsafe {
                *out_handle = handle_id.to_raw();
                *out_done = nros_completion_t { _slot: done.as_raw() };
            }
            exec.handle_count += 1;
            RCL_RET_OK
        }
        Err(e) => crate::support::report_node_refusal(
            "nros_executor_register_device",
            format_args!("device"),
            e,
        ),
    }
}
