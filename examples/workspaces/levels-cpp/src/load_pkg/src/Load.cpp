// Load.cpp - ws-levels-cpp load (10 Hz): burns `burn_us` of CPU per release.
//
// The burn reads the monotonic clock in a tight loop and counts only the
// steps shorter than 200 us as CPU time. A longer step is time the thread was
// off the CPU: preempted by a more urgent level, or throttled by the kernel
// once its SCHED_DEADLINE budget ran out. So a callback that the kernel
// throttles takes far longer in wall time than the CPU it burned, and its
// longest step is about one replenishment period. Each release prints, under
// the node's launch name,
//   [load] tick=N cpu_us=C wall_us=W max_gap_us=G
// (phase-162 W1 reads the throttle off it; W2 uses it as S1's telemetry load).

#include "load_pkg/Load.hpp"

#include <cstdio>

namespace load_pkg {

namespace {

constexpr uint64_t kStepNs = 200'000;

} // namespace

void Load::on_timer() {
    const uint64_t start = nros_cpp_time_ns();
    uint64_t prev = start;
    uint64_t cpu = 0;
    uint64_t max_gap = 0;
    while (cpu < burn_ns_) {
        const uint64_t now = nros_cpp_time_ns();
        const uint64_t step = now - prev;
        if (step < kStepNs) {
            cpu += step;
        } else if (step > max_gap) {
            max_gap = step;
        }
        prev = now;
    }
    const uint64_t wall = prev - start;
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    (void)pub_.publish(msg);
    std::printf("[%s] tick=%d cpu_us=%llu wall_us=%llu max_gap_us=%llu\n", get_name(), count_,
                static_cast<unsigned long long>(cpu / 1000u),
                static_cast<unsigned long long>(wall / 1000u),
                static_cast<unsigned long long>(max_gap / 1000u));
    count_++;
}

Load::Load(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "load") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // The CPU each release burns, from the launch file; the default is S1's
    // 5 ms telemetry callback.
    const int64_t burn_us = declare_parameter<int64_t>("burn_us", 5000);
    if (burn_us > 0) {
        burn_ns_ = static_cast<uint64_t>(burn_us) * 1000u;
    }
    std::printf("[%s] burn_us=%lld\n", get_name(), static_cast<long long>(burn_ns_ / 1000u));
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/levels/load");
    create_wall_timer_in<Load, &Load::on_timer>(100);
}

} // namespace load_pkg
