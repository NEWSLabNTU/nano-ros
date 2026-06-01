/// @file AddTwoIntsClient.c
/// @brief NuttX C AddTwoInts service client — Phase 212.L Component pkg.
///
/// Declarative metadata only; imperative call-sequencing is a runtime
/// follow-up wave (component model's TickCtx doesn't carry the service-
/// client call seam yet).

#include <nros/component.h>

static nros_ret_t register_service_client(nros_component_context_t *ctx) {
    nros_component_node_options_t opts =
        nros_component_node_options("add_two_ints_client");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t client = {
        .stable_id = "client_add",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_SERVICE_CLIENT,
        .source_name = "/add_two_ints",
        .type_name = "example_interfaces/srv/AddTwoInts",
        .type_hash = "",
        .callback_id = NULL,
    };
    return nros_component_create_entity(ctx, &client);
}

NROS_COMPONENT(register_service_client);
