/// @file FibonacciClient.c
/// @brief NuttX C Fibonacci action client — Phase 212.L Component pkg.
///
/// Declarative metadata only — imperative goal-sending is a runtime
/// follow-up.

#include <stddef.h>
#include <nros/node_pkg.h>

static nros_ret_t register_action_client(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("fibonacci_action_client");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t client;
    return nros_declared_node_create_action_client_for_name(
        &node, &client, "/fibonacci", "example_interfaces/action/Fibonacci", "");
}

NROS_NODE_REGISTER(register_action_client);
