// MrmHandler.cpp - ws-derived-tiers-cpp mrm_handler (10 Hz).
//
// One wall timer, one publisher. The 100 ms period is the code's own
// statement of the loop; the contract beside the launch file states the same
// loop as `paths.on_timer.trigger.timer.rate_hz: 10`, and that is what
// the tier derivation ranks by.

#include "mrm_handler_pkg/MrmHandler.hpp"

#include <cstdio>

namespace mrm_handler_pkg {

void MrmHandler::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[mrm_handler] tick=%d\n", count_);
    }
    count_++;
}

MrmHandler::MrmHandler(::nros::NodeHandle h)
    : ::nros::NodeWithTimers<1>(h, "mrm_handler") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/system/fail_safe/mrm_state");
    create_wall_timer_in<MrmHandler, &MrmHandler::on_timer>(100);
}

} // namespace mrm_handler_pkg
