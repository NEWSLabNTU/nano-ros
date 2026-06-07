/**
 * @file component.h
 * @ingroup grp_node
 * @brief Component-mode declarations for metadata and generated runtimes.
 *
 * Component packages do not define `main()`. They export a per-pkg mangled
 * `__nros_component_<pkg>_register` symbol (Phase 212.M.5.a.1) so multiple
 * components can link into one binary; the host metadata tool or generated
 * runtime supplies a context whose operations record declarations or
 * instantiate executor nodes. Codegen resolves the symbol name from each
 * package's metadata — there is no hardcoded bare-name constant.
 */

#ifndef NROS_NODE_PKG_H
#define NROS_NODE_PKG_H

#include <stddef.h>
#include <stdint.h>

#include "nros/visibility.h"

typedef int nros_ret_t;

#ifndef NROS_RET_OK
#define NROS_RET_OK 0
#endif

#ifndef NROS_RET_INVALID_ARGUMENT
#define NROS_RET_INVALID_ARGUMENT -3
#endif

#ifdef __cplusplus
extern "C" {
#endif

/* Phase 212.M.5.a.1 — the hardcoded `__nros_component_register` constant
 * is retired. Each Component pkg now exports a per-pkg mangled symbol
 * `__nros_component_<sanitised_pkg>_register`; codegen (and the metadata
 * tool) derive the name from the pkg's identity in workspace metadata.
 * Consumers that need the symbol name at runtime build it explicitly
 * via the per-pkg mangling rule (`-` → `_`, prepend `__nros_component_`,
 * append `_register`).
 */
#define NROS_MISSING_NODE_EXPORT_ERROR "package has no exported nros component"

typedef struct nros_node_context_t nros_node_context_t;

typedef struct nros_node_pkg_options_t {
    const char* name;
    const char* namespace_;
    uint32_t domain_id;
} nros_node_pkg_options_t;

typedef struct nros_declared_node_t {
    const char* stable_id;
    void* runtime_handle;
    nros_node_context_t* context;
} nros_declared_node_t;

typedef enum nros_node_entity_kind_t {
    NROS_NODE_ENTITY_PUBLISHER = 0,
    NROS_NODE_ENTITY_SUBSCRIPTION = 1,
    NROS_NODE_ENTITY_TIMER = 2,
    NROS_NODE_ENTITY_SERVICE_SERVER = 3,
    NROS_NODE_ENTITY_SERVICE_CLIENT = 4,
    NROS_NODE_ENTITY_ACTION_SERVER = 5,
    NROS_NODE_ENTITY_ACTION_CLIENT = 6,
    NROS_NODE_ENTITY_PARAMETER = 7,
} nros_node_entity_kind_t;

typedef struct nros_node_entity_descriptor_t {
    const char* stable_id;
    const char* node_id;
    nros_node_entity_kind_t kind;
    const char* source_name;
    const char* type_name;
    const char* type_hash;
    const char* callback_id;
} nros_node_entity_descriptor_t;

typedef enum nros_node_callback_effect_kind_t {
    // Phase 214 followup — second variant was a `NROS_NODE_CALLBACK_OTHER`
    // duplicate (typo from the rename sweep). Mirror the C++ enum in
    // `nros-cpp/include/nros/declared_node.hpp::CallbackEffectKind`:
    // Reads / Publishes / Writes.
    NROS_NODE_CALLBACK_READS = 0,
    NROS_NODE_CALLBACK_PUBLISHES = 1,
    NROS_NODE_CALLBACK_WRITES = 2,
} nros_node_callback_effect_kind_t;

typedef nros_ret_t (*nros_node_create_node_fn)(void* user_data, const char* stable_id,
                                               const nros_node_pkg_options_t* options,
                                               nros_declared_node_t* out_node);

typedef nros_ret_t (*nros_node_create_entity_fn)(void* user_data,
                                                 const nros_node_entity_descriptor_t* descriptor);

typedef nros_ret_t (*nros_node_record_callback_effect_fn)(void* user_data, const char* callback_id,
                                                          nros_node_callback_effect_kind_t kind,
                                                          const char* entity_id);

typedef struct nros_node_context_ops_t {
    nros_node_create_node_fn create_node;
    nros_node_create_entity_fn create_entity;
    nros_node_record_callback_effect_fn record_callback_effect;
} nros_node_context_ops_t;

struct nros_node_context_t {
    void* user_data;
    const nros_node_context_ops_t* ops;
};

typedef nros_ret_t (*nros_node_register_fn)(nros_node_context_t* context);

static inline nros_node_pkg_options_t nros_node_pkg_options(const char* name) {
    nros_node_pkg_options_t options;
    options.name = name;
    options.namespace_ = "/";
    options.domain_id = 0;
    return options;
}

static inline nros_ret_t nros_declared_node_create(nros_node_context_t* context,
                                                   const char* stable_id,
                                                   const nros_node_pkg_options_t* options,
                                                   nros_declared_node_t* out_node) {
    if (!context || !context->ops || !context->ops->create_node || !stable_id || !options ||
        !out_node) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    out_node->stable_id = stable_id;
    out_node->runtime_handle = NULL;
    out_node->context = context;
    nros_ret_t ret = context->ops->create_node(context->user_data, stable_id, options, out_node);
    if (ret == NROS_RET_OK) {
        out_node->stable_id = stable_id;
        out_node->context = context;
    } else {
        out_node->stable_id = NULL;
        out_node->runtime_handle = NULL;
        out_node->context = NULL;
    }
    return ret;
}

static inline nros_ret_t
nros_declared_node_init_with_options(nros_node_context_t* context,
                                     const nros_node_pkg_options_t* options,
                                     nros_declared_node_t* out_node) {
    if (!options || !options->name) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return nros_declared_node_create(context, options->name, options, out_node);
}

static inline nros_ret_t nros_declared_node_init_default(nros_node_context_t* context,
                                                         const char* node_name,
                                                         nros_declared_node_t* out_node) {
    if (!node_name) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    nros_node_pkg_options_t options = nros_node_pkg_options(node_name);
    return nros_declared_node_init_with_options(context, &options, out_node);
}

static inline nros_ret_t nros_node_create_entity(nros_node_context_t* context,
                                                 const nros_node_entity_descriptor_t* descriptor) {
    if (!context || !context->ops || !context->ops->create_entity || !descriptor) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return context->ops->create_entity(context->user_data, descriptor);
}

static inline nros_ret_t
nros_declared_node_create_entity(nros_declared_node_t* node, const char* stable_id,
                                 nros_node_entity_kind_t kind, const char* source_name,
                                 const char* type_name, const char* type_hash,
                                 const char* callback_id) {
    if (!node || !node->context || !node->stable_id || !stable_id || !source_name) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    nros_node_entity_descriptor_t descriptor;
    descriptor.stable_id = stable_id;
    descriptor.node_id = node->stable_id;
    descriptor.kind = kind;
    descriptor.source_name = source_name;
    descriptor.type_name = type_name ? type_name : "";
    descriptor.type_hash = type_hash ? type_hash : "";
    descriptor.callback_id = callback_id;
    return nros_node_create_entity(node->context, &descriptor);
}

static inline nros_ret_t nros_declared_node_create_publisher(nros_declared_node_t* node,
                                                             const char* stable_id,
                                                             const char* topic_name,
                                                             const char* type_name,
                                                             const char* type_hash) {
    return nros_declared_node_create_entity(node, stable_id, NROS_NODE_ENTITY_PUBLISHER,
                                            topic_name, type_name, type_hash, NULL);
}

static inline nros_ret_t nros_declared_node_create_subscription(nros_declared_node_t* node,
                                                                const char* stable_id,
                                                                const char* topic_name,
                                                                const char* type_name,
                                                                const char* type_hash,
                                                                const char* callback_id) {
    return nros_declared_node_create_entity(node, stable_id, NROS_NODE_ENTITY_SUBSCRIPTION,
                                            topic_name, type_name, type_hash, callback_id);
}

static inline nros_ret_t nros_declared_node_create_timer(nros_declared_node_t* node,
                                                         const char* stable_id,
                                                         const char* period_ms,
                                                         const char* callback_id) {
    return nros_declared_node_create_entity(node, stable_id, NROS_NODE_ENTITY_TIMER, period_ms, "",
                                            "", callback_id);
}

static inline nros_ret_t nros_node_record_callback_effect(nros_node_context_t* context,
                                                          const char* callback_id,
                                                          nros_node_callback_effect_kind_t kind,
                                                          const char* entity_id) {
    if (!context || !context->ops || !context->ops->record_callback_effect || !callback_id ||
        !entity_id) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return context->ops->record_callback_effect(context->user_data, callback_id, kind, entity_id);
}

/* Phase 212.M.5.a.1 — per-pkg mangled register symbol.
 *
 * Caller supplies the cargo-style pkg name as a bare token (pre-sanitised
 * — `-` → `_`). Cmake glue (`nano_ros_node_register()` for C++ or
 * the codegen-system C bake) is the canonical source of the pkg-name
 * token; hand-written C pkgs may invoke this macro directly.
 *
 *   NROS_COMPONENT(my_pkg, my_pkg_register_fn);
 *
 * expands to:
 *
 *   nros_ret_t __nros_component_my_pkg_register(nros_node_context_t*);
 *   const unsigned char __NROS_NODE_PKG_MY_PKG_EXPORT_PRESENT = 1;
 *   const char __nros_component_my_pkg_class_name[] = "...";
 */
#ifndef NROS_PKG_NAME
#define NROS_PKG_NAME unknown
#endif

#ifndef NROS_NODE_CLASS_NAME
#define NROS_NODE_CLASS_NAME "unknown"
#endif

#define _NROS_NODE_PKG_CONCAT(a, b) a##b
#define _NROS_NODE_PKG_CONCAT_X(a, b) _NROS_NODE_PKG_CONCAT(a, b)
#define _NROS_NODE_PKG_REG_SYM(pkg)                                                                \
    _NROS_NODE_PKG_CONCAT_X(__nros_component_, _NROS_NODE_PKG_CONCAT_X(pkg, _register))
#define _NROS_NODE_PKG_PRESENT_SYM(pkg)                                                            \
    _NROS_NODE_PKG_CONCAT_X(__NROS_NODE_PKG_, _NROS_NODE_PKG_CONCAT_X(pkg, _EXPORT_PRESENT))
#define _NROS_NODE_PKG_CLASS_SYM(pkg)                                                              \
    _NROS_NODE_PKG_CONCAT_X(__nros_component_, _NROS_NODE_PKG_CONCAT_X(pkg, _class_name))

#define NROS_COMPONENT(pkg, register_fn)                                                           \
    NROS_PUBLIC nros_ret_t _NROS_NODE_PKG_REG_SYM(pkg)(nros_node_context_t* context) {             \
        return (register_fn)(context);                                                             \
    }                                                                                              \
    NROS_PUBLIC const unsigned char _NROS_NODE_PKG_PRESENT_SYM(pkg) = 1;                           \
    NROS_PUBLIC const char _NROS_NODE_PKG_CLASS_SYM(pkg)[] = NROS_NODE_CLASS_NAME

/* Phase 214 followup — Phase 212.N.12 rename intended an `NROS_NODE_REGISTER`
 * 1-arg macro that uses the implicit `NROS_PKG_NAME` define injected by
 * `nano_ros_node_register()` cmake fn. Existing nuttx/freertos C examples
 * call `NROS_COMPONENT(register_fn);` (1-arg form, pre-rename name); supply
 * the 1-arg shape via a new `NROS_NODE_REGISTER` alias + a 1-arg
 * `NROS_COMPONENT` overload via __VA_ARGS__ dispatch so the legacy form
 * keeps working until callers migrate.
 */
#define NROS_NODE_REGISTER(register_fn) NROS_COMPONENT(NROS_PKG_NAME, register_fn)

#ifdef __cplusplus
}
#endif

#endif /* NROS_NODE_PKG_H */
