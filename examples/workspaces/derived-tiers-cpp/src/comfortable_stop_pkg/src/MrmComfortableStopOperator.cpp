// MrmComfortableStopOperator.cpp - ws-derived-tiers-cpp mrm_comfortable_stop_operator (10 Hz).
//
// One wall timer, one publisher. The 100 ms period is the code's own
// statement of the loop; the contract beside the launch file states the same
// loop as `paths.on_timer.trigger.timer.rate_hz: 10`, and that is what
// the tier derivation ranks by.

#include "comfortable_stop_pkg/MrmComfortableStopOperator.hpp"

#include <cstdio>

namespace comfortable_stop_pkg {

void MrmComfortableStopOperator::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[mrm_comfortable_stop_operator] tick=%d\n", count_);
    }
    count_++;
}

MrmComfortableStopOperator::MrmComfortableStopOperator(::nros::NodeHandle h)
    : ::nros::NodeWithTimers<1>(h, "mrm_comfortable_stop_operator") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/system/mrm/comfortable_stop/status");
    create_wall_timer_in<MrmComfortableStopOperator, &MrmComfortableStopOperator::on_timer>(100);
}

} // namespace comfortable_stop_pkg
