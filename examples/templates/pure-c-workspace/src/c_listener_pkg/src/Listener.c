#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_listener(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("listener");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t sub;
    r = nros_declared_node_create_subscription_for_name(&node, &sub, "/chatter",
                                                        "std_msgs/msg/Int32", "");
    if (r != NROS_RET_OK) return r;

    return nros_declared_entity_record_callback_effect(ctx, &sub, NROS_NODE_CALLBACK_READS, &sub);
}

NROS_NODE_REGISTER(register_listener);
