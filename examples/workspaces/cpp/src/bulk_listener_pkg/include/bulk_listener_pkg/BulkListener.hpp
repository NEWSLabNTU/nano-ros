#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace bulk_listener_pkg {

/// BulkListener — phase-441 W6's fragmented leg (issue 0917). Subscribes
/// `std_msgs/String` on `/bulk`, where the host publishes samples of ~10.6 KB:
/// eight RTPS fragments each, the island trajectory's shape. Each sample's
/// data opens with a decimal sequence number and a `:`; the callback prints
/// that number and the length it received, so the host side can tell how long
/// delivery lasted and that every sample arrived whole.
class BulkListener {
    unsigned long recv_ = 0;

    void on_msg(const ::std_msgs::msg::String& msg);

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace bulk_listener_pkg
