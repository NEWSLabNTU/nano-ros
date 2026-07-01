#pragma once

#include <cstdint>

#include <nros/component_node.hpp>

namespace ctrl_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path).
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 272 W3 (RFC-0047, issue #124) — rclcpp-shape high-tier control node.
/// IS-A `nros::ComponentNode`: receives the executor handle in the ctor and
/// creates a publisher + a 10 ms timer there. The entry placement-news it with
/// the live handle; the node's tier binding is resolved via the seeded
/// `node_name → sched_context` table (W1+W2) — no NodeHandle sched field needed.
class Ctrl : public ::nros::ComponentNode {
    ::nros::Publisher<Int32Tag> pub_;
    int count_ = 0;

    void on_tick();

  public:
    explicit Ctrl(::nros::NodeHandle h);
};

} // namespace ctrl_pkg
