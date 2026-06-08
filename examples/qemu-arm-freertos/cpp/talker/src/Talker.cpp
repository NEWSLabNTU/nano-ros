/// @file Talker.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ talker —
///        Phase 212.L Component pkg.

#include "Talker.hpp"

namespace freertos_cpp_talker {

::nros::Result Talker::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("talker");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredEntity pub;
    r = node.create_publisher(pub, "/chatter", "std_msgs/msg/Int32");
    if (!r.ok()) return r;

    ::nros::DeclaredCallback on_tick;
    r = node.declare_callback(on_tick, "on_tick");
    if (!r.ok()) return r;

    ::nros::DeclaredEntity timer;
    r = node.create_timer(timer, "1000", on_tick);
    if (!r.ok()) return r;

    return ctx.record_callback_effect(on_tick, ::nros::CallbackEffectKind::Publishes, pub);
}

} // namespace freertos_cpp_talker

NROS_NODE_REGISTER(freertos_cpp_talker::Talker, "freertos_cpp_talker::Talker");
