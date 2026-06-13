/// @file Talker.c
/// @brief ThreadX Linux C talker component — declarative Node pkg (Phase 244 D6).
///
/// Publishes `std_msgs/Int32` on `/chatter`. The generated runtime
/// (emitted by `nros_threadx_codegen_system`) owns init / executor / spin;
/// this file declares the node + publisher and exports the register
/// trampoline via `NROS_NODE_REGISTER`. Platform/RMW selection and the
/// connect locator live in the build (CMake) + board layers, never here.

#include <stddef.h>

#include <nros/node_pkg.h>

#include "std_msgs.h"

static nros_ret_t register_talker(nros_node_context_t *ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("c_talker");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t pub;
    return nros_declared_node_create_publisher_for_name(&node, &pub, "/chatter",
                                                        "std_msgs/msg/Int32", "");
}

NROS_NODE_REGISTER(register_talker);
