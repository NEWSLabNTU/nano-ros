/**
 * @file component.h
 * @ingroup grp_node
 * @brief Phase 240.4 (RFC-0043) — C stateful-component binding.
 *
 * The C analog of the C++ `nros/component.hpp`. A C component is a plain
 * `struct` (state) + a `configure` function that binds its real callbacks **by
 * identity** (function pointer + `self` context) on the real executor — no
 * string callback names, no synthesizing interpreter.
 *
 * The typed Entry (codegen `emit_typed` for a `lang=="c"` node, or the NuttX
 * typed C carrier) constructs the component via its factory and calls its
 * `configure`, handing it the node's FFI handle:
 *
 * ```c
 * typedef struct { int recv; } listener_t;
 *
 * static void on_raw(const uint8_t* data, size_t len, void* ctx) {
 *     listener_t* self = (listener_t*)ctx;
 *     // … decode + use self->recv …
 * }
 *
 * static nros_ret_t listener_configure(const nros_cpp_node_t* node, listener_t* self) {
 *     size_t h;
 *     return nros_cpp_subscription_register(node, "/chatter", "std_msgs/msg/Int32", "",
 *                                           nros_c_qos_default(), on_raw, self, 0, &h);
 * }
 *
 * NROS_C_COMPONENT(listener_t, listener_configure)  // emits create/configure exports
 * ```
 *
 * The bridge is the `nros_cpp_*` FFI: those symbols are C-ABI (the `cpp` is a
 * namespace prefix, not C++ linkage), so C calls them directly. The node handle
 * the Entry passes is exactly the one a C++ component would bind against, so a C
 * and a C++ component share the SAME executor + node.
 */

#ifndef NROS_COMPONENT_H
#define NROS_COMPONENT_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "nros/types.h" /* nros_ret_t */

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Opaque C++-side node FFI handle. The typed Entry passes the node's
 * `ffi_handle()` (a `const nros_cpp_node_t*`) to `configure` as this pointer;
 * the component forwards it to the raw-register FFI. C only ever holds the
 * pointer — never the struct's interior — so the forward declaration suffices.
 */
typedef struct nros_cpp_node_t nros_cpp_node_t;

/** Raw zero-copy subscription callback — `(data, len, ctx)`. ABI-identical to
 *  the C++ `nros_cpp_subscription_message_callback_t`. */
typedef void (*nros_c_subscription_callback_t)(const uint8_t* data, size_t len, void* ctx);

/* --- QoS mirror (layout-identical to the C++ `nros_cpp_qos_t`) ----------- */
enum nros_c_qos_reliability_t { NROS_C_QOS_RELIABLE = 0, NROS_C_QOS_BEST_EFFORT = 1 };
enum nros_c_qos_durability_t { NROS_C_QOS_VOLATILE = 0, NROS_C_QOS_TRANSIENT_LOCAL = 1 };
enum nros_c_qos_history_t { NROS_C_QOS_KEEP_LAST = 0, NROS_C_QOS_KEEP_ALL = 1 };
enum nros_c_qos_liveliness_t {
    NROS_C_QOS_LIVELINESS_NONE = 0,
    NROS_C_QOS_LIVELINESS_AUTOMATIC = 1,
    NROS_C_QOS_LIVELINESS_MANUAL_BY_TOPIC = 2,
    NROS_C_QOS_LIVELINESS_MANUAL_BY_NODE = 3
};

/** Layout-identical mirror of the C++ `nros_cpp_qos_t` (same field order/types),
 *  so passing it by value to `nros_cpp_subscription_register` is ABI-correct. */
typedef struct nros_cpp_qos_t {
    enum nros_c_qos_reliability_t reliability;
    enum nros_c_qos_durability_t durability;
    enum nros_c_qos_history_t history;
    enum nros_c_qos_liveliness_t liveliness_kind;
    int depth;
    uint32_t deadline_ms;
    uint32_t lifespan_ms;
    uint32_t liveliness_lease_ms;
    uint8_t avoid_ros_namespace_conventions;
} nros_cpp_qos_t;

/** Default profile: best-effort-free reliable, volatile, keep-last(10) — matches
 *  the C++ `QoS::default_profile()` the C++ components use. */
static inline nros_cpp_qos_t nros_c_qos_default(void) {
    nros_cpp_qos_t q;
    q.reliability = NROS_C_QOS_RELIABLE;
    q.durability = NROS_C_QOS_VOLATILE;
    q.history = NROS_C_QOS_KEEP_LAST;
    q.liveliness_kind = NROS_C_QOS_LIVELINESS_AUTOMATIC;
    q.depth = 10;
    q.deadline_ms = 0;
    q.lifespan_ms = 0;
    q.liveliness_lease_ms = 0;
    q.avoid_ros_namespace_conventions = 0;
    return q;
}

/**
 * Register a raw (zero-copy) subscription on the executor that owns `node`. The
 * callback borrows the wire bytes; `context` is carried through. C-ABI symbol
 * provided by nros-cpp (declared here for C; same signature as the C++ side).
 */
int32_t nros_cpp_subscription_register(const nros_cpp_node_t* node, const char* topic,
                                       const char* type_name, const char* type_hash,
                                       nros_cpp_qos_t qos, nros_c_subscription_callback_t callback,
                                       void* context, uint8_t sched_context, size_t* out_handle_id);

/** Raw callback-style service handler: receives the request bytes (`req`,
 *  `req_len`), fills the reply into `resp` (capacity `resp_cap`) + writes the
 *  byte count to `*resp_len`; returns `true` to send, `false` to drop. ABI-
 *  identical to the C++ `nros_cpp_service_request_callback_t`. */
typedef bool (*nros_c_service_request_callback_t)(const uint8_t* req, size_t req_len, uint8_t* resp,
                                                  size_t resp_cap, size_t* resp_len, void* ctx);

/**
 * Register a raw callback-style service server on the executor that owns `node`.
 * The handler runs during `spin_once`. C-ABI symbol provided by nros-cpp.
 */
int32_t nros_cpp_service_server_register(const nros_cpp_node_t* node, const char* service_name,
                                         const char* type_name, const char* type_hash,
                                         nros_cpp_qos_t qos,
                                         nros_c_service_request_callback_t callback, void* context,
                                         uint8_t sched_context, size_t* out_handle_id);

/* --- Factory / configure export macro ----------------------------------- */

#define NROS_C_PASTE_(a, b) a##b
#define NROS_C_PASTE(a, b) NROS_C_PASTE_(a, b)

/**
 * Emit the C-ABI factory + configure exports the typed Entry calls, keyed on
 * `NROS_PKG_NAME` (the sanitized pkg id the build passes as
 * `-DNROS_PKG_NAME=<pkg>` — the same token the C++ register macro keys on):
 *
 *   void*     __nros_c_component_<pkg>_create(void);    // -> &static instance
 *   nros_ret_t __nros_c_component_<pkg>_configure(const nros_cpp_node_t*, void*);
 *
 * `StructT` is the component state struct; `configure_fn` has signature
 * `nros_ret_t configure_fn(const nros_cpp_node_t* node, StructT* self)`. Storage
 * lives in this TU (no heap, no sizeof leak to the Entry).
 */
#define NROS_C_COMPONENT(StructT, configure_fn)                                                    \
    static StructT NROS_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                    \
    void* NROS_C_PASTE(NROS_C_PASTE(__nros_c_component_, NROS_PKG_NAME), _create)(void) {          \
        return &NROS_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                       \
    }                                                                                              \
    nros_ret_t NROS_C_PASTE(NROS_C_PASTE(__nros_c_component_, NROS_PKG_NAME),                      \
                            _configure)(const nros_cpp_node_t* node, void* self) {                 \
        return configure_fn(node, (StructT*)self);                                                 \
    }

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NROS_COMPONENT_H */
