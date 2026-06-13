/// @file ActionServer.c
/// @brief ThreadX Linux C action server component — declarative Node pkg (Phase 244 D6).
///
/// Serves `example_interfaces/Fibonacci` on `/fibonacci`. The generated
/// runtime (emitted by `nros_threadx_codegen_system`) owns init / executor /
/// spin; this file declares the node + action server and exports the register
/// trampoline via `NROS_NODE_REGISTER`.

#include <stddef.h>

#include <nros/node_pkg.h>

#include "example_interfaces.h"

static nros_ret_t register_action_server(nros_node_context_t *ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("c_action_server");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t act;
    return nros_declared_node_create_action_server_for_name(
        &node, &act, "/fibonacci", "example_interfaces/action/Fibonacci", "");
}

NROS_NODE_REGISTER(register_action_server);
