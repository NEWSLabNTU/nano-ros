// Ctrl.cpp — ws-realtime-cpp high-tier control node.
//
// Publishes a monotonic Int32 counter on /ctrl every 10 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization). The
// configure-shape receives a Node& that is already bound to the
// high-priority sched context (emitted by the C++ codegen path via
// NodeBuilder::sched).

#include "ctrl_pkg/Ctrl.hpp"

#include <cstdio>

#if defined(NROS_PLATFORM_NUTTX)
#include <time.h>
#endif

namespace ctrl_pkg {

#if defined(NROS_PLATFORM_NUTTX)
// Issue 2018 -- on NuttX the `high` level declares a 5 ms budget per 10 ms
// (`[tiers.high.nuttx]`), which the kernel's SCHED_SPORADIC server enforces.
// One tick in fifty runs for 8 ms, past the budget, so the kernel demotes the
// level to its floor priority and the runner reports it ("throttled by the
// KERNEL"); the e2e reads that line. The other ticks stay well inside it.
static void overrun_budget(int count) {
    if (count % 50 != 25) return;
    struct timespec t0;
    struct timespec t;
    ::clock_gettime(CLOCK_MONOTONIC, &t0);
    long elapsed_us = 0;
    do {
        ::clock_gettime(CLOCK_MONOTONIC, &t);
        elapsed_us = (long)(t.tv_sec - t0.tv_sec) * 1000000L + (t.tv_nsec - t0.tv_nsec) / 1000L;
    } while (elapsed_us < 8000L);
    std::printf("[ctrl] overran the level budget at tick=%d (%ld us)\n", count, elapsed_us);
}
#endif

void Ctrl::on_tick() {
#if defined(NROS_PLATFORM_NUTTX)
    overrun_budget(count_);
#endif
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[ctrl] tick=%d\n", count_);
    }
    count_++;
}

::rclcpp::Result Ctrl::configure(::rclcpp::Node& node) {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/ctrl");
    if (!r.ok()) return r;
    return node.create_wall_timer<Ctrl, &Ctrl::on_tick>(timer_, 10, this);
}

} // namespace ctrl_pkg
