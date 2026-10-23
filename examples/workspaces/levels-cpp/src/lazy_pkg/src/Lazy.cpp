// Lazy.cpp - ws-levels-cpp lazy (200 ms timer).
//
// Its callback RUNS for 60 ms between two markers, `[lazy] begin=N` and
// `[lazy] end=N`. On Zephyr the run is `k_busy_wait`: the thread stays
// ready, so only a MORE URGENT level's worker can take the CPU from it, and
// that level's own markers then print between this pair (phase-509 P9,
// RFC-0108 D3). Elsewhere (the host census of this image) it is a clock spin.

#include "lazy_pkg/Lazy.hpp"

#include <cstdint>
#include <cstdio>

#if defined(__ZEPHYR__)
#include <zephyr/kernel.h>
#else
#include <chrono>
#endif

namespace lazy_pkg {

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

void Lazy::on_timer() {
    std::printf("[lazy] begin=%d\n", count_);
    run_for_us(60000u);
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    (void)pub_.publish(msg);
    std::printf("[lazy] end=%d\n", count_);
    count_++;
}

Lazy::Lazy(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "lazy") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // phase-509 P9b -- the launch parameter seeded on this node by its
    // level's setup (system.launch.xml); the test reads it back here.
    const int64_t tag = declare_parameter<int64_t>("level_tag", -1);
    std::printf("[lazy] level_tag=%lld\n", static_cast<long long>(tag));
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/levels/lazy");
    create_wall_timer_in<Lazy, &Lazy::on_timer>(200);
}

} // namespace lazy_pkg
