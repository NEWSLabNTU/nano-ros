/**
 * @file component.h
 * @ingroup grp_node
 * @brief Component-mode declarations for metadata and generated runtimes.
 *
 * Component packages do not define `main()`. They export
 * `__nros_component_register`, and the host metadata tool or generated runtime
 * supplies a context whose operations record declarations or instantiate
 * executor nodes.
 */

#ifndef NROS_COMPONENT_H
#define NROS_COMPONENT_H

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

#define NROS_COMPONENT_EXPORT_SYMBOL "__nros_component_register"
#define NROS_MISSING_COMPONENT_EXPORT_ERROR "package has no exported nros component"

typedef struct nros_component_context_t nros_component_context_t;

typedef struct nros_component_node_options_t {
    const char* name;
    const char* namespace_;
    uint32_t domain_id;
} nros_component_node_options_t;

typedef struct nros_component_node_t {
    const char* stable_id;
    void* runtime_handle;
    nros_component_context_t* context;
} nros_component_node_t;

typedef enum nros_component_entity_kind_t {
    NROS_COMPONENT_ENTITY_PUBLISHER = 0,
    NROS_COMPONENT_ENTITY_SUBSCRIPTION = 1,
    NROS_COMPONENT_ENTITY_TIMER = 2,
    NROS_COMPONENT_ENTITY_SERVICE_SERVER = 3,
    NROS_COMPONENT_ENTITY_SERVICE_CLIENT = 4,
    NROS_COMPONENT_ENTITY_ACTION_SERVER = 5,
    NROS_COMPONENT_ENTITY_ACTION_CLIENT = 6,
    NROS_COMPONENT_ENTITY_PARAMETER = 7,
} nros_component_entity_kind_t;

typedef struct nros_component_entity_descriptor_t {
    const char* stable_id;
    const char* node_id;
    nros_component_entity_kind_t kind;
    const char* source_name;
    const char* type_name;
    const char* type_hash;
    const char* callback_id;
} nros_component_entity_descriptor_t;

typedef enum nros_component_callback_effect_kind_t {
    NROS_COMPONENT_CALLBACK_READS = 0,
    NROS_COMPONENT_CALLBACK_PUBLISHES = 1,
    NROS_COMPONENT_CALLBACK_WRITES = 2,
} nros_component_callback_effect_kind_t;

typedef nros_ret_t (*nros_component_create_node_fn)(void* user_data, const char* stable_id,
                                                    const nros_component_node_options_t* options,
                                                    nros_component_node_t* out_node);

typedef nros_ret_t (*nros_component_create_entity_fn)(
    void* user_data, const nros_component_entity_descriptor_t* descriptor);

typedef nros_ret_t (*nros_component_record_callback_effect_fn)(
    void* user_data, const char* callback_id, nros_component_callback_effect_kind_t kind,
    const char* entity_id);

typedef struct nros_component_context_ops_t {
    nros_component_create_node_fn create_node;
    nros_component_create_entity_fn create_entity;
    nros_component_record_callback_effect_fn record_callback_effect;
} nros_component_context_ops_t;

struct nros_component_context_t {
    void* user_data;
    const nros_component_context_ops_t* ops;
};

typedef nros_ret_t (*nros_component_register_fn)(nros_component_context_t* context);

static inline nros_component_node_options_t nros_component_node_options(const char* name) {
    nros_component_node_options_t options;
    options.name = name;
    options.namespace_ = "/";
    options.domain_id = 0;
    return options;
}

static inline nros_ret_t nros_component_create_node(nros_component_context_t* context,
                                                    const char* stable_id,
                                                    const nros_component_node_options_t* options,
                                                    nros_component_node_t* out_node) {
    if (!context || !context->ops || !context->ops->create_node || !stable_id || !options ||
        !out_node) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return context->ops->create_node(context->user_data, stable_id, options, out_node);
}

static inline nros_ret_t
nros_component_create_entity(nros_component_context_t* context,
                             const nros_component_entity_descriptor_t* descriptor) {
    if (!context || !context->ops || !context->ops->create_entity || !descriptor) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return context->ops->create_entity(context->user_data, descriptor);
}

static inline nros_ret_t
nros_component_record_callback_effect(nros_component_context_t* context, const char* callback_id,
                                      nros_component_callback_effect_kind_t kind,
                                      const char* entity_id) {
    if (!context || !context->ops || !context->ops->record_callback_effect || !callback_id ||
        !entity_id) {
        return NROS_RET_INVALID_ARGUMENT;
    }
    return context->ops->record_callback_effect(context->user_data, callback_id, kind, entity_id);
}

#define NROS_COMPONENT(register_fn)                                                                \
    NROS_PUBLIC nros_ret_t __nros_component_register(nros_component_context_t* context) {          \
        return (register_fn)(context);                                                             \
    }                                                                                              \
    NROS_PUBLIC const unsigned char __NROS_COMPONENT_EXPORT_PRESENT = 1

#ifdef __cplusplus
}
#endif

#endif /* NROS_COMPONENT_H */
