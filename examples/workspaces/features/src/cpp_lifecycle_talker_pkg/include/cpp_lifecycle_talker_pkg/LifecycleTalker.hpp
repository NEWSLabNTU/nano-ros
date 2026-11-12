#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace cpp_lifecycle_talker_pkg {

/// LifecycleTalker — Phase 269 W2: counter publisher on /chatter.
/// Lifecycle state machine (register + Configure→Activate) is handled by the
/// generated entry right after it builds the node, via
/// `nros_cpp_lifecycle_register_node`, because the launch file declares the
/// node as a `<lifecycle_node>`.
class LifecycleTalker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int32_t counter_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace cpp_lifecycle_talker_pkg
