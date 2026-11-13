// The C++ talker component (phase-553 W5), from the `nros new` talker template.
#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace baremetal_cpp_talker {

/// Talker — a typed component. `configure` creates a publisher on `/chatter`
/// and binds the member `on_tick` (by identity) as a 1 Hz timer callback.
class Talker {
    ::rclcpp::Publisher<std_msgs::msg::String> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace baremetal_cpp_talker
