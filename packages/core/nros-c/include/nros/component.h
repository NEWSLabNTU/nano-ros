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
 *                                           nros_c_qos_default(), on_raw, self, 0, &h, NULL);
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

/* phase-263 A4 — the C component storage buffers below MUST be at least as large
 * as the REAL per-build runtime structs. Those exact sizes live in the generated
 * `nros_cpp_config_generated.h` the build mirrors onto the include path (a hard
 * dep of every typed C component, which links nros-cpp). The static fallbacks
 * below are nuttx-era and have already drifted (native's action-server struct is
 * 120 bytes, not 80) — an under-sized buffer lets `nros_cpp_action_server_register`
 * overrun it and clobber the adjacent struct field (e.g. a stashed executor
 * handle → NULL → `complete_goal` returns INVALID_ARGUMENT). Pull the generated
 * sizes when the header is reachable; the mirrored real header wins over the
 * in-tree `#error` stub because the build lists the mirror dir first. */
#if defined(__has_include)
#if __has_include(<nros/nros_cpp_config_generated.h>)
#include <nros/nros_cpp_config_generated.h>
#endif
#endif

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

/* Phase 269 W3 — E2E integrity status for the validated-subscription callback.
 * ABI-identical to `nros_cpp_integrity_status_t` (nros_cpp_ffi.h). The guard
 * prevents double-definition when both headers are included. */
#ifndef NROS_CPP_FFI_H
typedef struct nros_cpp_integrity_status_t {
    /** Sequence-number gap since the previous in-order sample (0 = none). */
    int64_t gap;
    /** true if this sample's sequence number was already seen (a duplicate). */
    bool duplicate;
    /** CRC verdict: 1 = valid, 0 = mismatch, -1 = no CRC on the wire. */
    int8_t crc_valid;
} nros_cpp_integrity_status_t;
#endif /* NROS_CPP_FFI_H */

/** Phase 269 W3 — validated subscription callback: carries the CDR bytes AND the
 *  sample's E2E integrity status (CRC verdict + sequence gap/dup). The C/C++
 *  component-callback analog of Rust's `FnMut(&[u8], &IntegrityStatus)`.
 *  Requires `NANO_ROS_SAFETY_E2E=ON` (lowered from
 *  `[system].features = ["safety"]` via `NanoRosCapabilities.cmake`). */
typedef void (*nros_c_subscription_validated_callback_t)(const uint8_t* data, size_t len,
                                                         int64_t gap, bool duplicate,
                                                         int8_t crc_valid, void* ctx);

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
 *  so passing it by value to `nros_cpp_subscription_register` is ABI-correct.
 *  Guarded so a translation unit that ALSO includes `nros_cpp_ffi.h` (e.g. a C
 *  component that reads params) does not double-define the struct — ffi.h's
 *  definition wins when its header guard `NROS_CPP_FFI_H` is already set. */
#ifndef NROS_CPP_FFI_H
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
#endif /* NROS_CPP_FFI_H */

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
 *
 * `callback_group` (issue #129): the RFC-0047 callback-group name; NULL or ""
 * = the default group. Phase 273 appended this to the Rust FFI and the C++
 * header, but THIS C prototype was missed — C callers built against the 9-arg
 * shape left the 11th slot as stack garbage, which the Rust side dereferenced
 * (SIGSEGV in `cstr_to_str` on Zephyr native_sim; silent luck elsewhere).
 */
int32_t nros_cpp_subscription_register(const nros_cpp_node_t* node, const char* topic,
                                       const char* type_name, const char* type_hash,
                                       nros_cpp_qos_t qos, nros_c_subscription_callback_t callback,
                                       void* context, uint8_t sched_context, size_t* out_handle_id,
                                       const char* callback_group);

/**
 * Phase 269 W3 — Register a validated subscription: same as
 * `nros_cpp_subscription_register` but the callback ALSO receives the sample's
 * E2E integrity status (CRC verdict + sequence gap/dup) as three plain scalars
 * (gap, duplicate, crc_valid). The C/C++ component-callback analog of Rust's
 * `register_subscription_buffered_raw_safety_on` / `CallbackCtx::integrity()`.
 *
 * Requires `NANO_ROS_SAFETY_E2E=ON` (lowered from
 * `[system].features = ["safety"]` via `NanoRosCapabilities.cmake`). The
 * runtime validates the CRC the publisher attaches (automatic when built with
 * `safety-e2e`); `crc_valid` is `-1` if the publisher sent no CRC.
 */
int32_t nros_cpp_subscription_register_validated(const nros_cpp_node_t* node, const char* topic,
                                                 const char* type_name, const char* type_hash,
                                                 nros_cpp_qos_t qos,
                                                 nros_c_subscription_validated_callback_t callback,
                                                 void* context, uint8_t sched_context,
                                                 size_t* out_handle_id);

/* --- Publisher (raw) ---------------------------------------------------- */

#ifndef NROS_C_PUBLISHER_STORAGE_SIZE
#define NROS_C_PUBLISHER_STORAGE_SIZE 560
#endif

/** Create a publisher into the component-owned `storage`, then publish CDR bytes
 *  with `nros_cpp_publish_raw`. C-ABI symbols from nros-cpp. */
int32_t nros_cpp_publisher_create(const nros_cpp_node_t* node, const char* topic,
                                  const char* type_name, const char* type_hash, nros_cpp_qos_t qos,
                                  void* storage);
int32_t nros_cpp_publish_raw(void* storage, const uint8_t* data, size_t len);

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

/* --- Action server (executor-scoped; needs the `executor` configure arg) -- */

/** Storage sizes mirror the C++ `NROS_CPP_*` config values (nuttx defaults).
 *  The build may `-D` override; a C component declares an 8-aligned buffer of
 *  the right size for the transport it binds. */
#ifndef NROS_C_ACTION_SERVER_STORAGE_SIZE
#ifdef NROS_CPP_ACTION_SERVER_STORAGE_SIZE
#define NROS_C_ACTION_SERVER_STORAGE_SIZE NROS_CPP_ACTION_SERVER_STORAGE_SIZE
#else
#define NROS_C_ACTION_SERVER_STORAGE_SIZE 128
#endif
#endif
#ifndef NROS_C_SERVICE_CLIENT_STORAGE_SIZE
#define NROS_C_SERVICE_CLIENT_STORAGE_SIZE 4632
#endif

/** GoalResponse discriminant returned from the goal callback. */
enum nros_c_goal_response_t {
    NROS_C_GOAL_REJECT = 0,
    NROS_C_GOAL_ACCEPT_AND_EXECUTE = 1,
    NROS_C_GOAL_ACCEPT_AND_DEFER = 2
};
/** CancelResponse discriminant returned from the cancel callback. */
enum nros_c_cancel_response_t { NROS_C_CANCEL_REJECT = 0, NROS_C_CANCEL_ACCEPT = 1 };

/** Goal callback: receives the goal UUID + the goal's CDR bytes; returns a
 *  `nros_c_goal_response_t`. ABI-identical to `nros_cpp_goal_callback_t`. */
typedef int32_t (*nros_c_goal_callback_t)(const uint8_t goal_id[16], const uint8_t* data,
                                          size_t len, void* ctx);
/** Cancel callback: returns a `nros_c_cancel_response_t`. */
typedef int32_t (*nros_c_cancel_callback_t)(const uint8_t goal_id[16], void* ctx);

/* Raw action-server FFI (C-ABI symbols from nros-cpp; node + executor scoped). */
int32_t nros_cpp_action_server_create(const nros_cpp_node_t* node, const char* action_name,
                                      const char* type_name, const char* type_hash,
                                      nros_cpp_qos_t qos, void* storage);
int32_t nros_cpp_action_server_register(void* storage, void* executor_handle,
                                        const char* action_name, const char* type_name,
                                        const char* type_hash, uint8_t sched_context);
int32_t nros_cpp_action_server_set_callbacks(void* handle, nros_c_goal_callback_t goal_cb,
                                             nros_c_cancel_callback_t cancel_cb, void* ctx);
int32_t nros_cpp_action_server_complete_goal(void* handle, void* executor_handle,
                                             const uint8_t (*goal_id)[16],
                                             const uint8_t* result_buf, size_t result_len);

/* --- Timer (executor-scoped) -------------------------------------------- */

/** Timer callback — `(ctx)`. ABI-identical to `nros_cpp_timer_callback_t`. */
typedef void (*nros_c_timer_callback_t)(void* ctx);

/** Create + register a repeating timer on the executor (fires during spin_once).
 *  C-ABI symbol from nros-cpp. */
int32_t nros_cpp_timer_create(void* executor_handle, uint64_t period_ms,
                              nros_c_timer_callback_t callback, void* context,
                              size_t* out_handle_id);

/* --- Service client (poll model) ---------------------------------------- */

int32_t nros_cpp_service_client_create(const nros_cpp_node_t* node, const char* service_name,
                                       const char* type_name, const char* type_hash,
                                       nros_cpp_qos_t qos, void* storage);
int32_t nros_cpp_service_client_send_request(void* storage, const uint8_t* req_data,
                                             size_t req_len);
int32_t nros_cpp_service_client_try_recv_reply(void* storage, uint8_t* resp_data,
                                               size_t resp_capacity, size_t* resp_len);

/* --- Action client (poll model) ----------------------------------------- */

#ifndef NROS_C_ACTION_CLIENT_STORAGE_SIZE
#ifdef NROS_CPP_ACTION_CLIENT_STORAGE_SIZE
#define NROS_C_ACTION_CLIENT_STORAGE_SIZE NROS_CPP_ACTION_CLIENT_STORAGE_SIZE
#else
#define NROS_C_ACTION_CLIENT_STORAGE_SIZE 64
#endif
#endif

int32_t nros_cpp_action_client_create(const nros_cpp_node_t* node, const char* action_name,
                                      const char* type_name, const char* type_hash,
                                      nros_cpp_qos_t qos, void* storage);
/* ASYNC (non-blocking) goal/result — required from a poll client driven by a
 * timer callback: the *blocking* send_goal/get_result re-enter the executor from
 * inside spin_once and never complete. */
int32_t nros_cpp_action_client_send_goal_async(void* handle, const uint8_t* goal_buf,
                                               size_t goal_len, uint8_t (*goal_id_out)[16]);
int32_t nros_cpp_action_client_get_result_async(void* handle, const uint8_t (*goal_id)[16]);
int32_t nros_cpp_action_client_try_recv_goal_response(void* handle, uint8_t* out_data,
                                                      size_t out_capacity, size_t* out_len);
int32_t nros_cpp_action_client_try_recv_result(void* handle, uint8_t* out_data, size_t out_capacity,
                                               size_t* out_len);
/* Pump the action client's pending replies — a raw (non-arena-registered) client
 * must call this each spin cycle: it drains the GET-query replies and dispatches
 * them into the callbacks registered with set_callbacks. */
int32_t nros_cpp_action_client_poll(void* handle);

/* Callback dispatch (RFC-0041; issue-0047). ABI-identical to the C++ typedefs. */
typedef void (*nros_c_action_goal_response_callback_t)(bool accepted, const uint8_t goal_id[16],
                                                       void* ctx);
typedef void (*nros_c_action_feedback_callback_t)(const uint8_t goal_id[16], const uint8_t* data,
                                                  size_t len, void* ctx);
typedef void (*nros_c_action_result_callback_t)(const uint8_t goal_id[16], int32_t status,
                                                const uint8_t* data, size_t len, void* ctx);
int32_t nros_cpp_action_client_set_callbacks(void* handle,
                                             nros_c_action_goal_response_callback_t goal_response,
                                             nros_c_action_feedback_callback_t feedback,
                                             nros_c_action_result_callback_t result, void* context);

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
 * `nros_ret_t configure_fn(const nros_cpp_node_t* node, void* executor, StructT* self)`.
 * `executor` is the opaque executor handle (the C analog of
 * `Node::executor_handle()`) — needed for executor-scoped transports (action
 * server register / complete_goal); node-scoped binds (sub / service) ignore it.
 * Storage lives in this TU (no heap, no sizeof leak to the Entry).
 */
#define NROS_C_COMPONENT(StructT, configure_fn)                                                    \
    static StructT NROS_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                    \
    void* NROS_C_PASTE(NROS_C_PASTE(__nros_c_component_, NROS_PKG_NAME), _create)(void) {          \
        return &NROS_C_PASTE(__nros_c_inst_, NROS_PKG_NAME);                                       \
    }                                                                                              \
    nros_ret_t NROS_C_PASTE(NROS_C_PASTE(__nros_c_component_, NROS_PKG_NAME),                      \
                            _configure)(const nros_cpp_node_t* node, void* executor, void* self) { \
        return configure_fn(node, executor, (StructT*)self);                                       \
    }

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* NROS_COMPONENT_H */
