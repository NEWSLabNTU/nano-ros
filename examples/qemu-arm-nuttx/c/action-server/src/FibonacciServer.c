/// @file FibonacciServer.c
/// @brief NuttX C Fibonacci action server — Phase 212.L Component pkg.

#include <nros/node_pkg.h>

static nros_ret_t register_action_server(nros_node_context_t *ctx) {
    nros_node_pkg_options_t opts =
        nros_node_pkg_options("fibonacci_action_server");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_create(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t action = {
        .stable_id = "act_fib",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_ACTION_SERVER,
        .source_name = "/fibonacci",
        .type_name = "example_interfaces/action/Fibonacci",
        .type_hash = "",
        .callback_id = "on_goal",
    };
    return nros_node_create_entity(ctx, &action);
}

NROS_COMPONENT(register_action_server);
