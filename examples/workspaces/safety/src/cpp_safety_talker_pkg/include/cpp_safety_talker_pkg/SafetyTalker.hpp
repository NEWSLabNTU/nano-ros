#pragma once

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace cpp_safety_talker_pkg {

/// SafetyTalker — counter publisher on /chatter (std_msgs/Int32).
/// Message integrity is not declared here: the bringup's
/// `[system].features = ["safety"]` turns it on for the whole system, and the
/// runtime attaches the per-sample CRC on publish. See
/// `demo_bringup/system.toml`.
class SafetyTalker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int32_t counter_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace cpp_safety_talker_pkg
