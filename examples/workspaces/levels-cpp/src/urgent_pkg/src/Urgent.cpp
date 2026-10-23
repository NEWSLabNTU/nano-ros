// Urgent.cpp - ws-levels-cpp urgent (50 Hz), the most urgent level.
//
// Its callback only prints. The test reads WHERE its ticks land: inside a less
// urgent level's `begin=`/`end=` pair means this level's worker preempted that
// callback mid-run (phase-509 P9, RFC-0108 D3).

#include "urgent_pkg/Urgent.hpp"

#include <cstdint>
#include <cstdio>

namespace urgent_pkg {

void Urgent::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    (void)pub_.publish(msg);
    std::printf("[urgent] tick=%d\n", count_);
    count_++;
}

Urgent::Urgent(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "urgent") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // phase-509 P9b -- the launch parameter seeded on this node by its
    // level's setup (system.launch.xml); the test reads it back here.
    const int64_t tag = declare_parameter<int64_t>("level_tag", -1);
    std::printf("[urgent] level_tag=%lld\n", static_cast<long long>(tag));
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/levels/urgent");
    create_wall_timer_in<Urgent, &Urgent::on_timer>(20);
}

} // namespace urgent_pkg
