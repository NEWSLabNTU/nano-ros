#pragma once

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

namespace bulk_listener_pkg {

/// BulkListener — phase-441 W6's fragmented leg (issue 0917). Subscribes
/// `example_interfaces/String` on `/bulk`, where the host publishes samples of ~10.6 KB:
/// eight RTPS fragments each, the island trajectory's shape. Each sample's
/// data opens with a decimal sequence number and a `:`; the callback prints
/// that number and the length it received, so the host side can tell how long
/// delivery lasted and that every sample arrived whole.
///
/// `example_interfaces/String` rather than `std_msgs/String` so the 16 KiB bound
/// this sample needs (`nros-codegen.toml`) belongs to this package's type alone:
/// the tutorial talker and listener carry `std_msgs/String` at 256 bytes, and a
/// stock type's bound is workspace-wide (issue 1470).
class BulkListener {
    unsigned long recv_ = 0;

    void on_msg(const ::example_interfaces::msg::String& msg);

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace bulk_listener_pkg
