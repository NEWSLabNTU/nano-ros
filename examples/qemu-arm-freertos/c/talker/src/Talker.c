/// @file Talker.c
/// @brief FreeRTOS QEMU MPS2-AN385 C talker — Phase 212.L Component pkg.
///
/// Declarative register: node + publisher on /chatter + 1 Hz timer.
/// BSP-generated runtime owns init, executor, spin, and timer-callback
/// dispatch (M.5.a.3+4).

#include <nros/component.h>

#include "std_msgs.h"

static nros_ret_t register_talker(nros_component_context_t *ctx) {
    nros_component_node_options_t opts = nros_component_node_options("talker");
    nros_component_node_t node;
    nros_ret_t r = nros_component_create_node(ctx, "node", &opts, &node);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t pub = {
        .stable_id = "pub_chatter",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_PUBLISHER,
        .source_name = "/chatter",
        .type_name = "std_msgs/msg/Int32",
        .type_hash = "",
        .callback_id = NULL,
    };
    r = nros_component_create_entity(ctx, &pub);
    if (r != NROS_RET_OK) return r;

    nros_component_entity_descriptor_t timer = {
        .stable_id = "timer_tick",
        .node_id = "node",
        .kind = NROS_COMPONENT_ENTITY_TIMER,
        .source_name = "1000",
        .type_name = "",
        .type_hash = "",
        .callback_id = "on_tick",
    };
    r = nros_component_create_entity(ctx, &timer);
    if (r != NROS_RET_OK) return r;

    return nros_component_record_callback_effect(
        ctx, "on_tick", NROS_COMPONENT_CALLBACK_PUBLISHES, "pub_chatter");
}

NROS_COMPONENT(register_talker);
