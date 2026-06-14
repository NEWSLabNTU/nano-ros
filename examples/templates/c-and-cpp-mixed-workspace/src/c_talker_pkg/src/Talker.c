#include <stddef.h>

#include <nros/node_pkg.h>
#include "std_msgs.h"

static nros_ret_t register_talker(nros_node_context_t* ctx) {
    nros_declared_node_t node;
    nros_ret_t r = nros_declared_node_init_default(ctx, "talker", &node);
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t pub;
    r = nros_declared_node_create_publisher_for_name(&node, &pub, "/chatter", "std_msgs/msg/Int32",
                                                     "");
    if (r != NROS_RET_OK) return r;

    nros_declared_entity_t timer;
    r = nros_declared_node_create_timer_for_period(&node, &timer, "1000");
    if (r != NROS_RET_OK) return r;

    return nros_declared_entity_record_callback_effect(ctx, &timer, NROS_NODE_CALLBACK_PUBLISHES,
                                                       &pub);
}

NROS_NODE_REGISTER(register_talker);
