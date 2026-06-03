/// @file Listener.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ listener —
///        Phase 212.L Component pkg.

#include "Listener.hpp"

namespace freertos_cpp_listener {

::nros::Result Listener::register_component(::nros::ComponentContext& ctx) {
    ::nros::ComponentNode node;
    auto opts = ::nros::NodeOptions::make("listener");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::ComponentEntityDescriptor sub{
        "sub_chatter",
        "node",
        ::nros::ComponentEntityKind::Subscription,
        "/chatter",
        "std_msgs/msg/Int32",
        "",
        "on_chatter",
    };
    return node.create_entity(sub);
}

} // namespace freertos_cpp_listener

NROS_NODE_REGISTER(freertos_cpp_listener::Listener,
                        "freertos_cpp_listener::Listener");
