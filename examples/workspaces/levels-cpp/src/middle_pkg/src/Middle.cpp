// Middle.cpp - ws-levels-cpp middle (50 ms timer).
//
// Its callback RUNS for 15 ms between two markers, `[middle] begin=N` and
// `[middle] end=N`. On Zephyr the run is `k_busy_wait`: the thread stays
// ready, so only a MORE URGENT level's worker can take the CPU from it, and
// that level's own markers then print between this pair (phase-509 P9,
// RFC-0108 D3). Elsewhere (the host census of this image) it is a clock spin.

#include "middle_pkg/Middle.hpp"

#include <cstdint>
#include <cstdio>

#if defined(__ZEPHYR__)
#include <zephyr/kernel.h>
#else
#include <chrono>
#endif

namespace middle_pkg {

namespace {

void run_for_us(unsigned us) {
#if defined(__ZEPHYR__)
    k_busy_wait(us);
#else
    auto end = std::chrono::steady_clock::now() + std::chrono::microseconds(us);
    while (std::chrono::steady_clock::now() < end) {
    }
#endif
}

} // namespace

void Middle::on_timer() {
    std::printf("[middle] begin=%d\n", count_);
    run_for_us(15000u);
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    (void)pub_.publish(msg);
    std::printf("[middle] end=%d\n", count_);
    count_++;
}

Middle::Middle(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "middle") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // phase-509 P9b -- the launch parameter seeded on this node by its
    // level's setup (system.launch.xml); the test reads it back here.
    const int64_t tag = declare_parameter<int64_t>("level_tag", -1);
    std::printf("[middle] level_tag=%lld\n", static_cast<long long>(tag));
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/levels/middle");
    create_wall_timer_in<Middle, &Middle::on_timer>(50);
}

} // namespace middle_pkg
