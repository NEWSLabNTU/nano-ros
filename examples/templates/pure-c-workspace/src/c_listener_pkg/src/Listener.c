#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_listener(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("listener");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_create(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t sub = {
        .stable_id = "sub_chatter",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_SUBSCRIPTION,
        .source_name = "/chatter",
        .type_name = "std_msgs/msg/Int32",
        .type_hash = "",
        .callback_id = "on_message",
    };
    r = nros_node_create_entity(ctx, &sub);
    if (r != NROS_RET_OK) return r;

    return nros_node_record_callback_effect(ctx, "on_message", NROS_NODE_CALLBACK_READS,
                                            "sub_chatter");
}

NROS_NODE_REGISTER(register_listener);
