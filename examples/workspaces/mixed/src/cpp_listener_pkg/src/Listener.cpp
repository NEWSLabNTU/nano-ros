#include "Listener.hpp"

#include "std_msgs.hpp"

namespace cpp_listener_pkg {

::nros::Result Listener::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("listener");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor sub{
        "sub_chatter", "node", ::nros::NodeEntityKind::Subscription,
        "/chatter",    "std_msgs/msg/Int32", "", "on_message",
    };
    r = node.create_entity(sub);
    if (!r.ok()) return r;

    return ctx.record_callback_effect(
        "on_message", ::nros::CallbackEffectKind::Reads, "sub_chatter");
}

} // namespace cpp_listener_pkg

NROS_NODE_REGISTER(cpp_listener_pkg::Listener, "cpp_listener_pkg::Listener");
