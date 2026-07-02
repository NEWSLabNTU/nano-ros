#pragma once

#include <nros/component_node.hpp>

#include "std_msgs.hpp"

namespace telem_pkg {

/// rclcpp-shape low-tier telemetry node (RFC-0047, issue #124).
/// IS-A `nros::ComponentNode`: creates publisher + 100 ms timer in the ctor.
/// Tier binding resolved via the seeded `node_name → sched_context` table.
class Telem : public ::nros::ComponentNode {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_tick();

  public:
    explicit Telem(::nros::NodeHandle h);
};

} // namespace telem_pkg
