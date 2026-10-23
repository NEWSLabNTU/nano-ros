#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace middle_pkg {

/// phase-509 P9 - middle: the middle level: a 20 Hz timer whose callback runs 15 ms (k_busy_wait),
/// long enough for the most urgent level to preempt it.
class Middle : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_timer();

  public:
    explicit Middle(::rclcpp::NodeHandle h);
};

} // namespace middle_pkg
