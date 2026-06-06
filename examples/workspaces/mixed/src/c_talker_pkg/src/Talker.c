#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_talker(nros_node_context_t* ctx) {
    nros_node_pkg_options_t opts = nros_node_pkg_options("talker");
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_create(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t pub = {
        .stable_id = "pub_chatter",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_PUBLISHER,
        .source_name = "/chatter",
        .type_name = "std_msgs/msg/Int32",
        .type_hash = "",
        .callback_id = NULL,
    };
    r = nros_node_create_entity(ctx, &pub);
    if (r != NROS_RET_OK) return r;

    nros_node_entity_descriptor_t timer = {
        .stable_id = "timer_tick",
        .node_id = "node",
        .kind = NROS_NODE_ENTITY_TIMER,
        .source_name = "1000",
        .type_name = "",
        .type_hash = "",
        .callback_id = "on_tick",
    };
    r = nros_node_create_entity(ctx, &timer);
    if (r != NROS_RET_OK) return r;

    return nros_node_record_callback_effect(ctx, "on_tick", NROS_NODE_CALLBACK_PUBLISHES,
                                            "pub_chatter");
}

NROS_NODE_REGISTER(register_talker);
