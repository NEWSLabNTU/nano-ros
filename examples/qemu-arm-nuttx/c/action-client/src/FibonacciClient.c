/// @file FibonacciClient.c
/// @brief NuttX C Fibonacci action client — Phase 212.L Component pkg.
///
/// Declarative metadata only — imperative goal-sending is a runtime
/// follow-up.

#include <stddef.h>
#include <nros/node_pkg.h>

static nros_ret_t register_action_client(nros_node_context_t *ctx) {
    nros_node_pkg_options_t opts =
        nros_node_pkg_options("fibonacci_action_client");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_create(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t client = {
        .stable_id = "client_fib",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_ACTION_CLIENT,
        .source_name = "/fibonacci",
        .type_name = "example_interfaces/action/Fibonacci",
        .type_hash = "",
        .callback_id = NULL,
    };
    return nros_node_create_entity(ctx, &client);
}

NROS_NODE_REGISTER(register_action_client);
