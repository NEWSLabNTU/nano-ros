#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_talker(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("talker");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_with_options(ctx, &opts, &node);
    if (r != NROS_RET_OK) return r;

    r = nros_declared_node_create_publisher(&node, "pub_chatter", "/chatter",
                                            "std_msgs/msg/Int32", "");
    if (r != NROS_RET_OK) return r;

    r = nros_declared_node_create_timer(&node, "timer_tick", "1000", "on_tick");
    if (r != NROS_RET_OK) return r;

    return nros_node_record_callback_effect(ctx, "on_tick", NROS_NODE_CALLBACK_PUBLISHES,
                                            "pub_chatter");
}

NROS_NODE_REGISTER(register_talker);
