#pragma once

#include <cstddef>
#include <cstdint>

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace listener_pkg {

/// Listener — the ROS 2 tutorial listener as a stateful component (RFC-0043).
/// `configure` binds the member `on_msg` (by identity, no name) as a TYPED
/// member subscription on `/chatter` and prints the tutorial ready line; each
/// `std_msgs/String` sample prints `I heard: [<payload>]`. phase-535 made this
/// the ONE C++ listener (RFC-0122 D1, option B).
class Listener {
    int recv_ = 0;

    void on_msg(const ::std_msgs::msg::String& msg); // typed member callback

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace listener_pkg
