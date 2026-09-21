#pragma once

#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace mrm_handler_pkg {

/// phase-459 W0 - mrm_handler. IS-A node (rclcpp shape) with ONE wall timer at
/// 100 ms (10 Hz) publishing a counter on /system/fail_safe/mrm_state.
///
/// The component declares `CALLBACK_GROUPS main` in its CMakeLists and creates
/// no group in code: with exactly one declared group the whole node is placed
/// on the tier the rate-monotonic derivation assigns from the contract's
/// 10 Hz timer path. No tier, priority or period is written anywhere but the
/// timer call below and the contract beside the launch file.
class MrmHandler : public ::nros::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int count_ = 0;

    void on_timer();

  public:
    explicit MrmHandler(::nros::NodeHandle h);
};

} // namespace mrm_handler_pkg
