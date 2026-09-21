// StopModeOperator.cpp - ws-derived-tiers-cpp stop_mode_operator (30 Hz).
//
// One wall timer, one publisher. The 33 ms period is the code's own
// statement of the loop; the contract beside the launch file states the same
// loop as `paths.on_timer.trigger.timer.rate_hz: 30`, and that is what
// the tier derivation ranks by.

#include "stop_mode_pkg/StopModeOperator.hpp"

#include <cstdio>

namespace stop_mode_pkg {

void StopModeOperator::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[stop_mode_operator] tick=%d\n", count_);
    }
    count_++;
}

StopModeOperator::StopModeOperator(::nros::NodeHandle h)
    : ::nros::NodeWithTimers<1>(h, "stop_mode_operator") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/system/stop_mode/control");
    create_wall_timer_in<StopModeOperator, &StopModeOperator::on_timer>(33);
}

} // namespace stop_mode_pkg
