/// @file AddTwoIntsServer.c
/// @brief FreeRTOS QEMU MPS2-AN385 C AddTwoInts service server —
///        Phase 212.L Component pkg.

#include <stddef.h>
#include <nros/node_pkg.h>

static nros_ret_t register_service_server(nros_node_context_t *ctx) {
    nros_decl_node_options_t opts =
        nros_node_options("add_two_ints_server");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_create(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t srv = {
        .stable_id = "srv_add",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_SERVICE_SERVER,
        .source_name = "/add_two_ints",
        .type_name = "example_interfaces/srv/AddTwoInts",
        .type_hash = "",
        .callback_id = "handle_add",
    };
    return nros_node_create_entity(ctx, &srv);
}

NROS_NODE_REGISTER(register_service_server);
