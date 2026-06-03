/// @file Talker.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ talker —
///        Phase 212.L Component pkg.

#include "Talker.hpp"

namespace freertos_cpp_talker {

::nros::Result Talker::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("talker");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor pub{
        /*stable_id*/   "pub_chatter",
        /*node_id*/     "node",
        /*kind*/        ::nros::NodeEntityKind::Publisher,
        /*source_name*/ "/chatter",
        /*type_name*/   "std_msgs/msg/Int32",
        /*type_hash*/   "",
        /*callback_id*/ nullptr,
    };
    r = node.create_entity(pub);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor timer{
        "timer_tick", "node", ::nros::NodeEntityKind::Timer,
        "1000",       "",     "",                                "on_tick",
    };
    r = node.create_entity(timer);
    if (!r.ok()) return r;

    return ctx.record_callback_effect(
        "on_tick", ::nros::CallbackEffectKind::Publishes, "pub_chatter");
}

} // namespace freertos_cpp_talker

NROS_NODE_REGISTER(freertos_cpp_talker::Talker,
                        "freertos_cpp_talker::Talker");
