/**
 * @file rclc_parameter.h
 * @ingroup grp_parameter
 * @brief The C parameter server: rclc's API over the executor's store.
 *
 * RFC-0105 / phase-491. ONE parameter family for C, under rclc's own names and
 * at rclc's include path, so a ported `#include <rclc_parameter/rclc_parameter.h>`
 * finds it. Every parameter declared here is served by the six
 * `rcl_interfaces` services (`ros2 param list/get/set/describe/dump/load`).
 *
 * What differs from upstream rclc, and why (RFC-0105 "bounded envelope"):
 *
 *  - Nothing here allocates. The store is the executor's, sized at build time
 *    (`NROS_MAX_PARAMETERS`); `max_params` is a per-node quota inside it.
 *  - The node must be bound to an executor (`rclc_executor_node_init`) before
 *    `rclc_parameter_server_init_*`; otherwise `RCL_RET_NOT_INIT`.
 *  - Local typed sets fire the callback and skip read-only and range checks,
 *    as rclc's do. REMOTE sets enforce read-only AND the range rclc only
 *    advertises (rclcpp's behaviour).
 *  - `Parameter` has rosidl's member paths (`p->name.data`, `p->value.type`,
 *    `p->value.integer_value`, ...) but is not a rosidl ABI type; there are no
 *    `rcl_interfaces__msg__Parameter__init/fini`. Its pointers are valid only
 *    for the duration of the callback. A scalar `string_value` is
 *    NUL-terminated; array elements, including a string array's, are not —
 *    read `size`.
 *  - `notify_changed_over_dds` (default on) gives the node one
 *    `/parameter_events` publisher, and each accepted change is published
 *    on it (phase-491 W3, RFC-0105 D8).
 *
 * Calling context: the executor's task only, never an ISR. Like rclc, nothing
 * here takes a lock.
 *
 * Types are spelled with nano-ros's names (`rcl_ret_t`, `struct rcl_node_t`,
 * `struct rclc_executor_t`); `<rclc/detail/rcl_compat.h>` makes `rcl_ret_t`,
 * `rcl_node_t`, `rclc_executor_t` and the `RCL_RET_*` codes the same types, so
 * a ported file that includes it compiles unchanged. phase-492 makes the rcl
 * spellings primary across the C API.
 */

#ifndef RCLC_PARAMETER__RCLC_PARAMETER_H_
#define RCLC_PARAMETER__RCLC_PARAMETER_H_

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "rclc/types.h"

/* Issue 0865 — this surface exists only in an image built with the
 * `param-services` capability. Say so here rather than in `ld`'s output. */
#if defined(NROS_HAS_PARAM_SERVICES) && !NROS_HAS_PARAM_SERVICES
#error "<rclc_parameter/rclc_parameter.h> needs the param_services capability: declare \
features = [\"param_services\"] under [system] in system.toml, or \
set(NROS_FEATURES \"param_services\") before add_subdirectory(<nano-ros>), and rebuild."
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Return codes rclc adds (upstream values, including 40004)
 * =================================================================== */

#define RCLC_PARAMETER_MODIFICATION_REJECTED 4001
#define RCLC_PARAMETER_TYPE_MISMATCH 4002
#define RCLC_PARAMETER_DISABLED_ON_CALLBACK 40004

/** Handles a parameter server takes from `rclc_executor_init`'s count —
 *  rclc rolling's six services. Ours are not executor handles, so this only
 *  over-reserves; it is defined so ported handle arithmetic compiles. */
#define RCLC_EXECUTOR_PARAMETER_SERVER_HANDLES 6

/* ===================================================================
 * Types
 * =================================================================== */

/** rclc's parameter types (0..3), extended with the rest of
 *  `rcl_interfaces/msg/ParameterType` (4..9), which this server also serves. */
typedef enum rclc_parameter_type_t {
    RCLC_PARAMETER_NOT_SET = 0,
    RCLC_PARAMETER_BOOL = 1,
    RCLC_PARAMETER_INT = 2,
    RCLC_PARAMETER_DOUBLE = 3,
    RCLC_PARAMETER_STRING = 4,
    RCLC_PARAMETER_BYTE_ARRAY = 5,
    RCLC_PARAMETER_BOOL_ARRAY = 6,
    RCLC_PARAMETER_INT_ARRAY = 7,
    RCLC_PARAMETER_DOUBLE_ARRAY = 8,
    RCLC_PARAMETER_STRING_ARRAY = 9
} rclc_parameter_type_t;

/** rclc's options, field for field. */
typedef struct rclc_parameter_options_t {
    bool notify_changed_over_dds;
    size_t max_params;
    bool allow_undeclared_parameters;
    bool low_mem_mode;
} rclc_parameter_options_t;

/* rosidl's C runtime shapes, from their one definition site in nano-ros
 * (phase-492 W7). Each is guarded with upstream's include guard, so a
 * translation unit that already has rosidl's definition (same layout) keeps
 * it. */
#include <rosidl_runtime_c/primitives_sequence.h>
#include <rosidl_runtime_c/string.h>

#ifndef RCL_INTERFACES__MSG__DETAIL__PARAMETER_VALUE__STRUCT_H_
#define RCL_INTERFACES__MSG__DETAIL__PARAMETER_VALUE__STRUCT_H_
typedef struct rcl_interfaces__msg__ParameterValue {
    uint8_t type;
    bool bool_value;
    int64_t integer_value;
    double double_value;
    rosidl_runtime_c__String string_value;
    rosidl_runtime_c__octet__Sequence byte_array_value;
    rosidl_runtime_c__boolean__Sequence bool_array_value;
    rosidl_runtime_c__int64__Sequence integer_array_value;
    rosidl_runtime_c__double__Sequence double_array_value;
    rosidl_runtime_c__String__Sequence string_array_value;
} rcl_interfaces__msg__ParameterValue;
#endif

#ifndef RCL_INTERFACES__MSG__DETAIL__PARAMETER__STRUCT_H_
#define RCL_INTERFACES__MSG__DETAIL__PARAMETER__STRUCT_H_
typedef struct rcl_interfaces__msg__Parameter {
    rosidl_runtime_c__String name;
    rcl_interfaces__msg__ParameterValue value;
} rcl_interfaces__msg__Parameter;
#endif

/** rclc's short names. */
typedef rcl_interfaces__msg__Parameter Parameter;
typedef rcl_interfaces__msg__ParameterValue ParameterValue;

/** rclc's callback: `old_param` NULL when the write creates the parameter,
 *  `new_param` NULL when it deletes it; `false` refuses. Fired by local typed
 *  sets, remote sets, remote deletes (`NOT_SET`) and remote creations of an
 *  undeclared name — not by `rclc_add_parameter` or a local
 *  `rclc_delete_parameter`. While it runs, every mutating call on its server
 *  returns `RCLC_PARAMETER_DISABLED_ON_CALLBACK`. */
typedef bool (*rclc_parameter_callback_t)(const Parameter* old_param, const Parameter* new_param,
                                          void* context);

/** The server HANDLE. Its members are not rclc's (rclc's hold the parameter
 *  list; ours is in the executor), so treat it as opaque. Zero-initialise it
 *  before init, and keep it at one address until `rclc_parameter_server_fini`. */
typedef struct rclc_parameter_server_t {
    void* _executor;
    size_t _max_params;
    rclc_parameter_callback_t _on_modification;
    void* _context;
    uint8_t _node_id;
    bool _initialized;
    bool _on_callback;
    bool _has_hook;
    uint16_t _hook;
    bool _notify_changed_over_dds;
    bool _low_mem_mode;
} rclc_parameter_server_t;

/* ===================================================================
 * rclc's functions
 * =================================================================== */

/** Defaults: events on, 4 parameters, undeclared sets refused. */
NROS_PUBLIC rcl_ret_t rclc_parameter_server_init_default(rclc_parameter_server_t* parameter_server,
                                                         struct rcl_node_t* node);
NROS_PUBLIC rcl_ret_t rclc_parameter_server_init_with_option(
    rclc_parameter_server_t* parameter_server, struct rcl_node_t* node,
    const rclc_parameter_options_t* options);
/** Unregisters the callback and deletes the node's parameters. */
NROS_PUBLIC rcl_ret_t rclc_parameter_server_fini(rclc_parameter_server_t* parameter_server,
                                                 struct rcl_node_t* node);

/** Registers the six parameter services and the callback (NULL = none).
 *  `executor` must be the one the server's node is bound to. */
NROS_PUBLIC rcl_ret_t rclc_executor_add_parameter_server(struct rclc_executor_t* executor,
                                                         rclc_parameter_server_t* parameter_server,
                                                         rclc_parameter_callback_t on_modification);
NROS_PUBLIC rcl_ret_t rclc_executor_add_parameter_server_with_context(
    struct rclc_executor_t* executor, rclc_parameter_server_t* parameter_server,
    rclc_parameter_callback_t on_modification, void* context);

/** Declares `type`'s zero value. A launch value already baked under this name
 *  with this type is kept. */
NROS_PUBLIC rcl_ret_t rclc_add_parameter(rclc_parameter_server_t* parameter_server,
                                         const char* parameter_name, rclc_parameter_type_t type);
NROS_PUBLIC rcl_ret_t rclc_delete_parameter(rclc_parameter_server_t* parameter_server,
                                            const char* parameter_name);

NROS_PUBLIC rcl_ret_t rclc_parameter_set_bool(rclc_parameter_server_t* parameter_server,
                                              const char* parameter_name, bool value);
NROS_PUBLIC rcl_ret_t rclc_parameter_set_int(rclc_parameter_server_t* parameter_server,
                                             const char* parameter_name, int64_t value);
NROS_PUBLIC rcl_ret_t rclc_parameter_set_double(rclc_parameter_server_t* parameter_server,
                                                const char* parameter_name, double value);
NROS_PUBLIC rcl_ret_t rclc_parameter_get_bool(rclc_parameter_server_t* parameter_server,
                                              const char* parameter_name, bool* output);
NROS_PUBLIC rcl_ret_t rclc_parameter_get_int(rclc_parameter_server_t* parameter_server,
                                             const char* parameter_name, int64_t* output);
NROS_PUBLIC rcl_ret_t rclc_parameter_get_double(rclc_parameter_server_t* parameter_server,
                                                const char* parameter_name, double* output);

NROS_PUBLIC rcl_ret_t rclc_add_parameter_description(rclc_parameter_server_t* parameter_server,
                                                     const char* parameter_name,
                                                     const char* parameter_description,
                                                     const char* additional_constraints);
/** Refuses REMOTE writes; the owner's local sets still go through. */
NROS_PUBLIC rcl_ret_t rclc_set_parameter_read_only(rclc_parameter_server_t* parameter_server,
                                                   const char* parameter_name, bool read_only);
/** Advertised by `describe` and enforced on REMOTE writes. */
NROS_PUBLIC rcl_ret_t rclc_add_parameter_constraint_integer(
    rclc_parameter_server_t* parameter_server, const char* parameter_name, int64_t from_value,
    int64_t to_value, uint64_t step);
NROS_PUBLIC rcl_ret_t rclc_add_parameter_constraint_double(
    rclc_parameter_server_t* parameter_server, const char* parameter_name, double from_value,
    double to_value, double step);

/* ===================================================================
 * Extensions (nano-ros; rclc has no equivalent)
 * =================================================================== */

/** Declare with a default, keeping a launch value of the same type (rclcpp's
 *  `declare_parameter`). Prefer these over add-then-set, which overwrites it. */
NROS_PUBLIC rcl_ret_t rclc_parameter_declare_bool(rclc_parameter_server_t* parameter_server,
                                                  const char* parameter_name, bool default_value);
NROS_PUBLIC rcl_ret_t rclc_parameter_declare_int(rclc_parameter_server_t* parameter_server,
                                                 const char* parameter_name, int64_t default_value);
NROS_PUBLIC rcl_ret_t rclc_parameter_declare_double(rclc_parameter_server_t* parameter_server,
                                                    const char* parameter_name,
                                                    double default_value);
NROS_PUBLIC rcl_ret_t rclc_parameter_declare_string(rclc_parameter_server_t* parameter_server,
                                                    const char* parameter_name,
                                                    const char* default_value);

NROS_PUBLIC rcl_ret_t rclc_parameter_set_string(rclc_parameter_server_t* parameter_server,
                                                const char* parameter_name, const char* value);
/** `RCLC_RET_FULL` when `capacity` (NUL included) is too small; nothing is
 *  written then. */
NROS_PUBLIC rcl_ret_t rclc_parameter_get_string(rclc_parameter_server_t* parameter_server,
                                                const char* parameter_name, char* output,
                                                size_t capacity);

/* Arrays, bounded by NROS_MAX_ARRAY_LEN (bytes: NROS_MAX_BYTE_ARRAY_LEN). A get
 * writes the stored length to `*out_len` and returns `RCLC_RET_FULL` without
 * copying when it exceeds `capacity`. */
#define NROS__RCLC_ARRAY(NAME, T)                                                                  \
    NROS_PUBLIC rcl_ret_t rclc_parameter_declare_##NAME##_array(                                   \
        rclc_parameter_server_t* parameter_server, const char* parameter_name, const T* data,      \
        size_t len);                                                                               \
    NROS_PUBLIC rcl_ret_t rclc_parameter_set_##NAME##_array(                                       \
        rclc_parameter_server_t* parameter_server, const char* parameter_name, const T* data,      \
        size_t len);                                                                               \
    NROS_PUBLIC rcl_ret_t rclc_parameter_get_##NAME##_array(                                       \
        rclc_parameter_server_t* parameter_server, const char* parameter_name, T* output,          \
        size_t capacity, size_t* out_len);
NROS__RCLC_ARRAY(bool, bool)
NROS__RCLC_ARRAY(int, int64_t)
NROS__RCLC_ARRAY(double, double)
NROS__RCLC_ARRAY(byte, uint8_t)
#undef NROS__RCLC_ARRAY

/* String arrays: `data` is `len` C strings, each at most NROS_MAX_STRING_VALUE_LEN
 * bytes. A get copies into `capacity` rows of `stride` bytes, each
 * NUL-terminated, writes the stored length to `*out_len`, and returns
 * `RCLC_RET_FULL` without copying when the rows or a row are too small. */
NROS_PUBLIC rcl_ret_t rclc_parameter_declare_string_array(rclc_parameter_server_t* parameter_server,
                                                          const char* parameter_name,
                                                          const char* const* data, size_t len);
NROS_PUBLIC rcl_ret_t rclc_parameter_set_string_array(rclc_parameter_server_t* parameter_server,
                                                      const char* parameter_name,
                                                      const char* const* data, size_t len);
NROS_PUBLIC rcl_ret_t rclc_parameter_get_string_array(rclc_parameter_server_t* parameter_server,
                                                      const char* parameter_name, char* output,
                                                      size_t stride, size_t capacity,
                                                      size_t* out_len);

/** `RCLC_PARAMETER_NOT_SET` when undeclared. */
NROS_PUBLIC rclc_parameter_type_t rclc_parameter_get_type(rclc_parameter_server_t* parameter_server,
                                                          const char* parameter_name);
/** Any out pointer may be NULL. `RCLC_RET_FULL` when a text was truncated (it
 *  is still written, NUL-terminated). */
NROS_PUBLIC rcl_ret_t rclc_parameter_describe(rclc_parameter_server_t* parameter_server,
                                              const char* parameter_name, char* out_description,
                                              size_t description_capacity, char* out_constraints,
                                              size_t constraints_capacity, bool* out_read_only);
/** `RCL_RET_ERROR` when no range of that kind is attached. */
NROS_PUBLIC rcl_ret_t rclc_parameter_get_integer_range(rclc_parameter_server_t* parameter_server,
                                                       const char* parameter_name,
                                                       int64_t* out_from, int64_t* out_to,
                                                       int64_t* out_step);
NROS_PUBLIC rcl_ret_t rclc_parameter_get_double_range(rclc_parameter_server_t* parameter_server,
                                                      const char* parameter_name, double* out_from,
                                                      double* out_to, double* out_step);
/** Names starting with `prefix` (NULL = all) into `max_names` rows of
 *  `name_stride` bytes. `*out_count` always receives the total; pass
 *  `max_names` 0 to count. `RCLC_RET_FULL` when more matched than fit. */
NROS_PUBLIC rcl_ret_t rclc_parameter_list(rclc_parameter_server_t* parameter_server,
                                          const char* prefix, char* out_names, size_t name_stride,
                                          size_t max_names, size_t* out_count);

#ifdef __cplusplus
}
#endif

#endif /* RCLC_PARAMETER__RCLC_PARAMETER_H_ */
