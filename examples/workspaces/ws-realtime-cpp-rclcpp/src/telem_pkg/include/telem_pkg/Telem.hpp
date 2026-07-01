#pragma once

#include <cstdint>

#include <nros/component_node.hpp>

namespace telem_pkg {

/// Minimal type tag for std_msgs/Int32 (raw-CDR path).
struct Int32Tag {
    static constexpr const char *TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char *TYPE_HASH = "";
};

/// Phase 272 W3 (RFC-0047, issue #124) — rclcpp-shape low-tier telemetry node.
/// IS-A `nros::ComponentNode`: creates publisher + 100 ms timer in the ctor.
/// Tier binding resolved via the seeded `node_name → sched_context` table (W1+W2).
class Telem : public ::nros::ComponentNode {
    ::nros::Publisher<Int32Tag> pub_;
    int count_ = 0;

    void on_tick();

  public:
    explicit Telem(::nros::NodeHandle h);
};

} // namespace telem_pkg
