#pragma once

#include <nros/component_node.hpp>

#include "std_msgs.hpp"

namespace ctrl_pkg {

/// rclcpp-shape high-tier control node (RFC-0047, issue #124).
/// IS-A `nros::ComponentNode`: receives the executor handle in the ctor and
/// creates a publisher + a 10 ms timer there. The entry placement-news it with
/// the live handle; the node's tier binding is resolved via the seeded
/// `node_name → sched_context` table — no NodeHandle sched field needed.
class Ctrl : public ::nros::ComponentNode {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_tick();

  public:
    explicit Ctrl(::nros::NodeHandle h);
};

} // namespace ctrl_pkg
