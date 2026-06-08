/// @file AddTwoIntsServer.c
/// @brief FreeRTOS QEMU MPS2-AN385 C AddTwoInts service server —
///        Phase 212.L Component pkg.

#include <stddef.h>
#include <nros/node_pkg.h>

static nros_ret_t register_service_server(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("add_two_ints_server");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t srv;
    return nros_declared_node_create_service_server_for_name(
        &node, &srv, "/add_two_ints", "example_interfaces/srv/AddTwoInts", "");
}

NROS_NODE_REGISTER(register_service_server);
