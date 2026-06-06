#pragma once

#include <nros/node_pkg.hpp>

namespace listener_pkg {

/// Listener Node — declares a subscription on `/chatter`. The Entry
/// pkg's planner instantiates each declared entity; `on_message` fires
/// when the sibling `talker_pkg` publishes.
class Listener {
  public:
    static ::nros::Result register_node(::nros::NodeContext& ctx);
};

} // namespace listener_pkg
