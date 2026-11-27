#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace lc_cpp_talker {

/// A managed C++ talker: a counter on /chatter (std_msgs/Int32) every 1 s.
/// The launch file marks it `<lifecycle_node>`; it holds no lifecycle code,
/// because the runtime gates its publisher on the node's state.
class Talker {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Timer timer_;
    int32_t counter_ = 0;

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace lc_cpp_talker
