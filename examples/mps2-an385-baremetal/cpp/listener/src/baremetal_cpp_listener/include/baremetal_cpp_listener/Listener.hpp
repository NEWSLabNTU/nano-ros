#pragma once

#include <cstddef>
#include <cstdint>

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace baremetal_cpp_listener {

/// Listener — the ROS 2 tutorial listener as a stateful component (RFC-0043).
/// `configure` binds the member `on_msg` (by identity, no name) as a TYPED
/// member subscription on `/chatter` and prints the tutorial ready line; each
/// `std_msgs/String` sample prints `I heard: [<payload>]`. A copy of `examples/workspaces/cpp`'s
/// component of the same role: a bare-metal image needs its own configure (issue 2555).
class Listener {
    int recv_ = 0;

    void on_msg(const ::std_msgs::msg::String& msg); // typed member callback

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace baremetal_cpp_listener
