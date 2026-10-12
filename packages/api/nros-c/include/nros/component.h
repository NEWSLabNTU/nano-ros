/**
 * @file component.h
 * @ingroup grp_node
 * @brief Phase 240.4 (RFC-0043) — C stateful-component binding.
 *
 * The C analog of the C++ `nros/component.hpp`. A C component is a plain
 * `struct` (state) + a `configure` function that creates its entities with the
 * C API and binds its callbacks **by identity** (function pointer + `self`
 * context) — no string callback names, no synthesizing interpreter.
 *
 * The generated entry (codegen `emit_typed` for a `lang=="c"` node, or the
 * NuttX typed C carrier) creates the node itself — with the launch plan's
 * namespace, scheduling, remaps and QoS overrides — constructs the component
 * via its factory, and calls its `configure` with the C API's handles:
 *
 * ```c
 * typedef struct {
 *     rcl_subscription_t sub;
 *     std_msgs__msg__Int32 msg;
 *     int recv;
 * } listener_t;
 *
 * static void on_msg(const void* msgin, void* ctx) {
 *     listener_t* self = (listener_t*)ctx;
 *     const std_msgs__msg__Int32* msg = (const std_msgs__msg__Int32*)msgin;
 *     self->recv = msg->data;
 * }
 *
 * static rcl_ret_t listener_configure(rcl_node_t* node, rclc_executor_t* executor,
 *                                     listener_t* self) {
 *     self->sub = rcl_get_zero_initialized_subscription();
 *     rcl_ret_t rc = rclc_subscription_init_default(
 *         &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
 *     if (rc != RCL_RET_OK) return rc;
 *     return rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg,
 *                                                        on_msg, self, RCLC_EXECUTOR_ON_NEW_DATA);
 * }
 *
 * RCLC_C_COMPONENT(listener_t, listener_configure)  // emits create/configure exports
 * ```
 *
 * phase-492 W4 — the handles are REAL C API objects over the host's own node
 * and executor, not copies: the C++ binding's context embeds an
 * `rclc_executor_t` and runs its executor inside it, and
 * `nros_cpp_node_bind_c` fills an executor-bound `rcl_node_t` for the node the
 * entry created. A C component therefore calls the C API only, a C++ and a C
 * component in one image share one executor and one node table, and the
 * `nros_cpp_*` FFI stays the C++ binding's.
 */

#ifndef NROS_COMPONENT_H
#define NROS_COMPONENT_H

#include <stddef.h>
#include <stdint.h>

#include "nros/types.h" /* rcl_ret_t, rcl_node_t, rclc_executor_t */
/* phase-454 W10 — `NROS_ASSERT_DECLARED_DEPTH`, for a C component that states a
 * QoS depth at a call site. The header is self-guarding and C-only, so a C++ TU
 * that pulls this in (the cross-include syntax check does) sees nothing and
 * keeps `<nros/declared_qos.hpp>`. */
#include "nros/declared_qos.h"

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Opaque handle of the node the generated entry created. Only the component
 * seam below sees it; a component never does.
 */
typedef struct nros_cpp_node_t nros_cpp_node_t;

/**
 * phase-492 W4 — fill `node_out` (an `rcl_node_t *`) with the C handle of
 * `node` and set `*executor_out` to the host's executor as an
 * `rclc_executor_t *`. Defined by the runtime (`nros-cpp`); called only by
 * `RCLC_C_COMPONENT`'s generated `configure`. Returns 0 on success.
 */
int32_t nros_cpp_node_bind_c(const nros_cpp_node_t* node, void* node_out, void** executor_out);

/* --- Factory / configure export macro ----------------------------------- */

#define RCLC_C_PASTE_(a, b) a##b
#define RCLC_C_PASTE(a, b) RCLC_C_PASTE_(a, b)

/**
 * Emit the C-ABI factory + configure exports the generated entry calls, keyed
 * on `NROS_PKG_NAME` (the sanitized pkg id the build passes as
 * `-DNROS_PKG_NAME=<pkg>` — the same token the C++ register macro keys on):
 *
 *   void*     __nros_c_component_<pkg>_create(void);    // -> &static instance
 *   rcl_ret_t __nros_c_component_<pkg>_configure(const nros_cpp_node_t*, void*, void*);
 *
 * `StructT` is the component state struct; `configure_fn` has signature
 * `rcl_ret_t configure_fn(rcl_node_t* node, rclc_executor_t* executor, StructT* self)`.
 * The component's state and its `rcl_node_t` live in this TU as statics (no
 * heap, no sizeof leak to the entry), so every handle the component creates
 * stays valid for as long as the host spins.
 */
#define RCLC_C_COMPONENT(StructT, configure_fn)                                                    \
    static StructT RCLC_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                    \
    static rcl_node_t RCLC_C_PASTE(__nros_c_node_, NROS_PKG_NAME);                                 \
    void* RCLC_C_PASTE(RCLC_C_PASTE(__nros_c_component_, NROS_PKG_NAME), _create)(void) {          \
        return &RCLC_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                       \
    }                                                                                              \
    rcl_ret_t RCLC_C_PASTE(RCLC_C_PASTE(__nros_c_component_, NROS_PKG_NAME),                       \
                           _configure)(const nros_cpp_node_t* node, void* executor, void* self) {  \
        rclc_executor_t* rclc_executor = NULL;                                                     \
        (void)executor; /* the host's context; the node names its executor */                      \
        if (nros_cpp_node_bind_c(node, &RCLC_C_PASTE(__nros_c_node_, NROS_PKG_NAME),               \
                                 (void**)&rclc_executor) != 0) {                                   \
            return RCL_RET_INVALID_ARGUMENT;                                                       \
        }                                                                                          \
        return configure_fn(&RCLC_C_PASTE(__nros_c_node_, NROS_PKG_NAME), rclc_executor,           \
                            (StructT*)self);                                                       \
    }

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NROS_COMPONENT_H */
