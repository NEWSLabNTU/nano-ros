// Talker — Phase 212.L.9 declarative Node pkg shape.
//
// `register_node()` describes one node + one publisher on `/chatter` +
// one 1 Hz timer firing `on_tick`. The Entry pkg's planner (post-219.D)
// instantiates each declared entity and dispatches `on_tick` to the
// runtime-generated trampoline. Sibling Node pkg `listener_pkg`
// subscribes to the same topic.

#include "Talker.hpp"
#include "std_msgs.hpp"

namespace talker_pkg {

::nros::Result Talker::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("talker");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredEntity publisher;
    r = node.create_publisher<std_msgs::msg::Int32>(publisher, "/chatter");
    if (!r.ok()) return r;

    ::nros::DeclaredCallback on_tick;
    r = node.declare_callback(on_tick, "on_tick");
    if (!r.ok()) return r;

    ::nros::DeclaredEntity timer;
    r = node.create_timer(timer, "1000", on_tick);
    if (!r.ok()) return r;

    return ctx.record_callback_effect(on_tick, ::nros::CallbackEffectKind::Publishes, publisher);
}

} // namespace talker_pkg

NROS_NODE_REGISTER(talker_pkg::Talker, "talker_pkg::Talker");
