/// @file AddTwoIntsClient.c
/// @brief NuttX C AddTwoInts service client — Phase 212.L Component pkg.
///
/// Declarative metadata only; imperative call-sequencing is a runtime
/// follow-up wave (component model's TickCtx doesn't carry the service-
/// client call seam yet).

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
