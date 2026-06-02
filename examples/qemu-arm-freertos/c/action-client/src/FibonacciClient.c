/// @file FibonacciClient.c
/// @brief FreeRTOS QEMU MPS2-AN385 C Fibonacci action client —
///        Phase 212.L Component pkg.
///
/// Phase 212.M.5.b — declarative-metadata-only.
/// Service-client runtime body deferred to M-F.4 (TickCtx call() seam) —
/// the same dependency applies to action-client send_goal /
/// feedback-stream wiring.

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
