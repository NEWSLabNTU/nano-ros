#include "Listener.hpp"

#include "std_msgs.hpp"

namespace cpp_listener_pkg {

::nros::Result Listener::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("listener");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredCallback on_message;
    r = node.declare_callback(on_message, "on_message");
    if (!r.ok()) return r;

    ::nros::DeclaredEntity subscription;
    r = node.create_subscription<std_msgs::msg::Int32>(subscription, "/chatter", on_message);
    if (!r.ok()) return r;

    return ctx.record_callback_effect(on_message, ::nros::CallbackEffectKind::Reads, subscription);
}

} // namespace cpp_listener_pkg

NROS_NODE_REGISTER(cpp_listener_pkg::Listener, "cpp_listener_pkg::Listener");
