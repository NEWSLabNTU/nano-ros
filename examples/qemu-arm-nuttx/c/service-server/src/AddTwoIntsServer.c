/// @file AddTwoIntsServer.c
/// @brief NuttX C AddTwoInts service server — Phase 212.L Component pkg.

#include <nros/component.h>

static nros_ret_t register_service_server(nros_component_context_t *ctx) {
    nros_component_node_options_t opts =
        nros_component_node_options("add_two_ints_server");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t srv = {
        .stable_id = "srv_add",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_SERVICE_SERVER,
        .source_name = "/add_two_ints",
        .type_name = "example_interfaces/srv/AddTwoInts",
        .type_hash = "",
        .callback_id = "handle_add",
    };
    return nros_component_create_entity(ctx, &srv);
}

NROS_COMPONENT(register_service_server);
