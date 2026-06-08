/// @file Listener.cpp
/// @brief NuttX C++ listener — Phase 212.L Component pkg.

#include "Listener.hpp"

namespace nuttx_cpp_listener {

::nros::Result Listener::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("listener");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredCallback on_chatter;
    r = node.declare_callback(on_chatter, "on_chatter");
    if (!r.ok()) return r;

    ::nros::DeclaredEntity sub;
    return node.create_subscription(sub, "/chatter", "std_msgs/msg/Int32", on_chatter);
}

} // namespace nuttx_cpp_listener

NROS_NODE_REGISTER(nuttx_cpp_listener::Listener, "nuttx_cpp_listener::Listener");
