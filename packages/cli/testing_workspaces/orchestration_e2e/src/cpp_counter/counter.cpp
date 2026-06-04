#include <cstdint>

using nros_ret_t = std::int32_t;

struct nros_component_context_t;

struct nros_component_node_options_t {
    const char* name;
    const char* namespace_;
    std::uint32_t domain_id;
};

struct nros_component_node_t {
    const char* stable_id;
    void* runtime_handle;
    nros_component_context_t* context;
};

using nros_component_create_node_fn = nros_ret_t (*)(void* user_data, const char* stable_id,
                                                     const nros_component_node_options_t* options,
                                                     nros_component_node_t* out_node);

struct nros_component_context_ops_t {
    nros_component_create_node_fn create_node;
    void* create_entity;
    void* record_callback_effect;
};

struct nros_component_context_t {
    void* user_data;
    const nros_component_context_ops_t* ops;
};

extern "C" nros_ret_t nros_component_cpp_counter(nros_component_context_t* context) {
    nros_component_node_t node{};
    nros_component_node_options_t options{};
    options.name = "cpp_counter";
    options.namespace_ = "/cpp";
    options.domain_id = 0;
    return context->ops->create_node(context->user_data, "cpp_counter_node", &options, &node);
}
