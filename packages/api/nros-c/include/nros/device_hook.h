/*
 * The device extension hook: register a device event source on an executor.
 *
 * This is an extension surface for board code and the device companion
 * package, not part of the ROS user API, so it lives under nros/ and never
 * under rclc/. A node author reaches devices through the companion package,
 * which builds its worker and source registration on this call.
 */
#ifndef NROS_DEVICE_HOOK_H
#define NROS_DEVICE_HOOK_H

#include <stddef.h>

#include <nros/device_port.h>
#include <rclc/rclc.h>

#ifdef __cplusplus
extern "C" {
#endif

/** The continuation a device runs when its completion is signalled. */
typedef void (*nros_device_callback_t)(void* context);

/**
 * Register a device event source on `executor`, before its first spin.
 *
 * Claims one callback slot. Its handle is written to `out_handle`, which
 * `rclc_executor_bind_handle_to_sched_context` takes to give the
 * continuation a priority. The completion handle the device's port signals
 * is written to `out_done`.
 *
 * The handle's pointer is the executor's own state for this slot: it stays
 * valid for the executor's whole life, so a port may store it and signal it
 * from an interrupt handler long after registration. Signal it with
 * `nros_completion_signal` from a thread, or `nros_completion_signal_from_isr`
 * from an interrupt handler (`<nros/device_port.h>`).
 *
 * `callback` and `context` must stay valid for the executor's life.
 *
 * Returns RCL_RET_OK; RCL_RET_INVALID_ARGUMENT for a null argument or
 * callback; or an error when the executor's callback table is full.
 */
rcl_ret_t nros_executor_register_device(rclc_executor_t* executor, nros_device_callback_t callback,
                                        void* context, size_t* out_handle,
                                        nros_completion_t* out_done);

#ifdef __cplusplus
}
#endif

#endif /* NROS_DEVICE_HOOK_H */
