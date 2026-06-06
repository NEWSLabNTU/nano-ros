#pragma once

#include <nros/node_pkg.hpp>

namespace talker_pkg {

/// Talker Node — declares a 1 Hz publisher on `/chatter`. The Entry
/// pkg's planner instantiates each declared entity; the timer fires
/// `on_tick`, which publishes a counter.
class Talker {
  public:
    static ::nros::Result register_node(::nros::NodeContext& ctx);
};

} // namespace talker_pkg
