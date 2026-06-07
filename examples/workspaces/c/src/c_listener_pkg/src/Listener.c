#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_listener(nros_node_context_t* ctx) {
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_default(ctx, "listener", &node);
    if (r != NROS_RET_OK) return r;

    r = nros_declared_node_create_subscription(
        &node, "sub_chatter", "/chatter", "std_msgs/msg/Int32", "", "on_message");
    if (r != NROS_RET_OK) return r;

    return nros_node_record_callback_effect(ctx, "on_message", NROS_NODE_CALLBACK_READS,
                                            "sub_chatter");
}

NROS_NODE_REGISTER(register_listener);
