#include <stdint.h>

typedef int32_t nros_ret_t;

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

typedef nros_ret_t (*nros_component_create_node_fn)(void* user_data, const char* stable_id,
                                                    const nros_component_node_options_t* options,
                                                    nros_component_node_t* out_node);

typedef struct nros_component_context_ops_t {
    nros_component_create_node_fn create_node;
    void* create_entity;
    void* record_callback_effect;
} nros_component_context_ops_t;

struct nros_component_context_t {
    void* user_data;
    const nros_component_context_ops_t* ops;
};

nros_ret_t nros_component_counter(nros_component_context_t* context) {
    nros_component_node_t node = {0};
    nros_component_node_options_t options = {
        .name = "counter",
        .namespace_ = "/c",
        .domain_id = 0,
    };
    return context->ops->create_node(context->user_data, "counter_node", &options, &node);
}
