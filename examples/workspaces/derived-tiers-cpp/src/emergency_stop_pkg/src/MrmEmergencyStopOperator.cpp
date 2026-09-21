// MrmEmergencyStopOperator.cpp - ws-derived-tiers-cpp mrm_emergency_stop_operator (30 Hz).
//
// One wall timer, one publisher. The 33 ms period is the code's own
// statement of the loop; the contract beside the launch file states the same
// loop as `paths.on_timer.trigger.timer.rate_hz: 30`, and that is what
// the tier derivation ranks by.

#include "emergency_stop_pkg/MrmEmergencyStopOperator.hpp"

#include <cstdio>

namespace emergency_stop_pkg {

void MrmEmergencyStopOperator::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[mrm_emergency_stop_operator] tick=%d\n", count_);
    }
    count_++;
}

MrmEmergencyStopOperator::MrmEmergencyStopOperator(::nros::NodeHandle h)
    : ::nros::NodeWithTimers<1>(h, "mrm_emergency_stop_operator") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/system/emergency/control_cmd");
    create_wall_timer_in<MrmEmergencyStopOperator, &MrmEmergencyStopOperator::on_timer>(33);
}

} // namespace emergency_stop_pkg
