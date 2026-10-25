#pragma once

#include <rclcpp/rclcpp.hpp>

#include <cstdint>

#include "std_msgs.hpp"

namespace attitude_pkg {

/// phase-162 W2 - RFC-0002 S1's critical loop: a 1 kHz timer (1 ms deadline)
/// whose callback works for 300 us. Every 1000 releases it prints one window
/// line with how many release gaps were longer than period plus deadline.
class Attitude : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;
    int window_ = 0;
    uint64_t last_ns_ = 0;
    uint32_t misses_ = 0;
    uint64_t worst_gap_ns_ = 0;

    void on_timer();

  public:
    explicit Attitude(::rclcpp::NodeHandle h);
};

} // namespace attitude_pkg
