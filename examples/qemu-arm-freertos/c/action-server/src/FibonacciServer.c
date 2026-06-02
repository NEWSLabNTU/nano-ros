/// @file FibonacciServer.c
/// @brief FreeRTOS QEMU MPS2-AN385 C Fibonacci action server —
///        Phase 212.L Component pkg.

#include <nros/component.h>

static nros_ret_t register_action_server(nros_component_context_t *ctx) {
    nros_component_node_options_t opts =
        nros_component_node_options("fibonacci_action_server");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t action = {
        .stable_id = "act_fib",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_ACTION_SERVER,
        .source_name = "/fibonacci",
        .type_name = "example_interfaces/action/Fibonacci",
        .type_hash = "",
        .callback_id = "on_goal",
    };
    return nros_component_create_entity(ctx, &action);
}

NROS_COMPONENT(register_action_server);
