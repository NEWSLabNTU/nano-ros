#pragma once

#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace stop_mode_pkg {

/// phase-459 W0 - stop_mode_operator. IS-A node (rclcpp shape) with ONE wall timer at
/// 33 ms (30 Hz) publishing a counter on /system/stop_mode/control.
///
/// The component declares `CALLBACK_GROUPS main` in its CMakeLists and creates
/// no group in code: with exactly one declared group the whole node is placed
/// on the tier the rate-monotonic derivation assigns from the contract's
/// 30 Hz timer path. No tier, priority or period is written anywhere but the
/// timer call below and the contract beside the launch file.
class StopModeOperator : public ::nros::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_timer();

  public:
    explicit StopModeOperator(::nros::NodeHandle h);
};

} // namespace stop_mode_pkg
