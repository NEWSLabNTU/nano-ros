/// @file Listener.cpp
/// @brief NuttX C++ listener — Phase 212.L Component pkg.

#include "Listener.hpp"

namespace nuttx_cpp_listener {

::nros::Result Listener::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("listener");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor sub{
        "sub_chatter",
        "node",
        ::nros::NodeEntityKind::Subscription,
        "/chatter",
        "std_msgs/msg/Int32",
        "",
        "on_chatter",
    };
    return node.create_entity(sub);
}

} // namespace nuttx_cpp_listener

NROS_NODE_REGISTER(nuttx_cpp_listener::Listener,
                        "nuttx_cpp_listener::Listener");
