// Listener — Phase 212.L.9 declarative Node pkg shape.
//
// `register_node()` describes one node + one subscription on `/chatter`,
// wiring the `on_message` callback to consume Int32 samples published by
// the sibling `talker_pkg`. The Entry pkg's planner (post-219.D)
// instantiates each declared entity and dispatches `on_message` on every
// incoming sample.

#include "Listener.hpp"
#include "std_msgs.hpp"

namespace listener_pkg {

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

} // namespace listener_pkg

NROS_NODE_REGISTER(listener_pkg::Listener, "listener_pkg::Listener");
