/// @file Listener.c
/// @brief FreeRTOS QEMU MPS2-AN385 C listener — Phase 212.L Component pkg.
///
/// Declarative: node + Int32 subscription on /chatter with `on_chatter`
/// callback. BSP-generated runtime owns init / executor / spin and the
/// callback-dispatch trampoline.

#include <nros/component.h>

#include "std_msgs.h"

static nros_ret_t register_listener(nros_component_context_t *ctx) {
    nros_component_node_options_t opts = nros_component_node_options("listener");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t sub = {
        .stable_id = "sub_chatter",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_SUBSCRIPTION,
        .source_name = "/chatter",
        .type_name = "std_msgs/msg/Int32",
        .type_hash = "",
        .callback_id = "on_chatter",
    };
    return nros_component_create_entity(ctx, &sub);
}

NROS_COMPONENT(register_listener);
