/// @file ServiceClient.c
/// @brief ThreadX Linux C service client component — declarative Node pkg (Phase 244 D6).
///
/// Calls `example_interfaces/AddTwoInts` on `/add_two_ints`. The generated
/// runtime (emitted by `nros_threadx_codegen_system`) owns init / executor /
/// spin; this file declares the node + service client and exports the register
/// trampoline via `NROS_NODE_REGISTER`.

#include <stddef.h>

#include <nros/node_pkg.h>

#include "example_interfaces.h"

static nros_ret_t register_service_client(nros_node_context_t *ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("c_service_client");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t cli;
    return nros_declared_node_create_service_client_for_name(
        &node, &cli, "/add_two_ints", "example_interfaces/srv/AddTwoInts", "");
}

NROS_NODE_REGISTER(register_service_client);
