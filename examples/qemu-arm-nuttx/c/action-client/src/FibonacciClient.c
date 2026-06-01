/// @file FibonacciClient.c
/// @brief NuttX C Fibonacci action client — Phase 212.L Component pkg.
///
/// Declarative metadata only — imperative goal-sending is a runtime
/// follow-up.

#include <nros/component.h>

static nros_ret_t register_action_client(nros_component_context_t *ctx) {
    nros_component_node_options_t opts =
        nros_component_node_options("fibonacci_action_client");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t client = {
        .stable_id = "client_fib",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_ACTION_CLIENT,
        .source_name = "/fibonacci",
        .type_name = "example_interfaces/action/Fibonacci",
        .type_hash = "",
        .callback_id = NULL,
    };
    return nros_component_create_entity(ctx, &client);
}

NROS_COMPONENT(register_action_client);
