#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace lazy_pkg {

/// phase-509 P9 - lazy: the least urgent level: a 5 Hz timer whose callback runs 60 ms
/// (k_busy_wait), so both more urgent levels preempt it.
class Lazy : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_timer();

  public:
    explicit Lazy(::rclcpp::NodeHandle h);
};

} // namespace lazy_pkg
