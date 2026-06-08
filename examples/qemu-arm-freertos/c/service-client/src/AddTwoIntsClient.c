/// @file AddTwoIntsClient.c
/// @brief FreeRTOS QEMU MPS2-AN385 C AddTwoInts service client —
///        Phase 212.L Component pkg.
///
/// Phase 212.M.5.b — declarative-metadata-only.
/// Service-client runtime body deferred to M-F.4 (TickCtx call() seam).

#include <stddef.h>
#include <nros/node_pkg.h>

static nros_ret_t register_service_client(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("add_two_ints_client");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t client;
    return nros_declared_node_create_service_client_for_name(
        &node, &client, "/add_two_ints", "example_interfaces/srv/AddTwoInts", "");
}

NROS_NODE_REGISTER(register_service_client);
