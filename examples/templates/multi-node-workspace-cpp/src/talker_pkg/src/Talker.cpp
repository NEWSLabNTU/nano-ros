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

    r = node.create_publisher("pub_chatter", "/chatter", "std_msgs/msg/Int32");
    if (!r.ok()) return r;

    r = node.create_timer("timer_tick", "1000", "on_tick");
    if (!r.ok()) return r;

    return ctx.record_callback_effect(
        "on_tick", ::nros::CallbackEffectKind::Publishes, "pub_chatter");
}

} // namespace talker_pkg

NROS_NODE_REGISTER(talker_pkg::Talker, "talker_pkg::Talker");
