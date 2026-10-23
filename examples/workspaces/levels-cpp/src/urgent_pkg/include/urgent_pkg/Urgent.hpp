#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace urgent_pkg {

/// phase-509 P9 - urgent: the most urgent level: a 50 Hz timer that only prints, so its tick lands
/// inside a less urgent level's running callback when it preempts it.
class Urgent : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_timer();

  public:
    explicit Urgent(::rclcpp::NodeHandle h);
};

} // namespace urgent_pkg
