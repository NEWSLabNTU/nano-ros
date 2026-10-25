// Attitude.cpp - ws-levels-cpp attitude (1 kHz), RFC-0002 S1's critical loop.
//
// A release is due every 1 ms and must finish within 1 ms of it. Two
// consecutive callback starts more than 2 ms apart (period plus deadline) mean
// at least one release in between finished past its deadline, so the count is
// a LOWER bound on deadline misses: it never reports a miss that did not
// happen, and the negative control (s1_flat.launch.xml) shows it can see one.
//
// Every 1000 releases it prints, under the node's launch name,
//   [attitude] window=W ticks=1000 misses=M worst_gap_us=G
// which the test sums over the run (phase-162 W2).

#include "attitude_pkg/Attitude.hpp"

#include <cstdio>

namespace attitude_pkg {

namespace {

constexpr uint64_t kPeriodNs = 1'000'000;
constexpr uint64_t kDeadlineNs = 1'000'000;
constexpr uint64_t kWorkNs = 300'000;
constexpr int kWindow = 1000;

} // namespace

void Attitude::on_timer() {
    const uint64_t start = nros_cpp_time_ns();
    if (last_ns_ != 0) {
        const uint64_t gap = start - last_ns_;
        if (gap > kPeriodNs + kDeadlineNs) {
            misses_++;
        }
        if (gap > worst_gap_ns_) {
            worst_gap_ns_ = gap;
        }
    }
    last_ns_ = start;
    while (nros_cpp_time_ns() - start < kWorkNs) {
    }
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    (void)pub_.publish(msg);
    count_++;
    if (count_ % kWindow == 0) {
        std::printf("[%s] window=%d ticks=%d misses=%u worst_gap_us=%llu\n", get_name(), window_,
                    kWindow, static_cast<unsigned>(misses_),
                    static_cast<unsigned long long>(worst_gap_ns_ / 1000u));
        window_++;
        misses_ = 0;
        worst_gap_ns_ = 0;
    }
}

Attitude::Attitude(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "attitude") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher_in<std_msgs::msg::Int32>("/levels/attitude");
    create_wall_timer_in<Attitude, &Attitude::on_timer>(1);
}

} // namespace attitude_pkg
