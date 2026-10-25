#pragma once

#include <rclcpp/rclcpp.hpp>

#include <cstdint>

#include "std_msgs.hpp"

namespace load_pkg {

/// phase-162 W1/W2 - a 10 Hz timer whose callback burns `burn_us` of CPU (the
/// launch parameter), counting only the time it was on the CPU. RFC-0002 S1's
/// best-effort telemetry in s1.launch.xml, and the level that runs past its
/// kernel budget in budget.launch.xml.
class Load : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;
    uint64_t burn_ns_ = 5'000'000;

    void on_timer();

  public:
    explicit Load(::rclcpp::NodeHandle h);
};

} // namespace load_pkg
