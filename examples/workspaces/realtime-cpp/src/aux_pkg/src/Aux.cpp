// Aux.cpp — realtime-cpp mid-sched-task auxiliary node.
//
// Publishes a monotonic Int32 counter on /aux every 50 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization). Runs on the `mid`
// sched task (RFC-0015 Model 1, one sched task per sched task). Which sched task is the bringup's
// call, not this node's — `demo_bringup/system.toml` carries
// `group_tasks = { aux = "mid" }`, and the `[tasks.mid.*]` blocks beside it say
// what `mid` means on each platform. The mid sched task is spawned by a spawned sched task
// (boot→mid→low) — a `[aux] tick` proves the #144 chained spawn serialized the
// declares so this sched task's publisher write filter opened.

#include "aux_pkg/Aux.hpp"

#include <cstdio>

namespace aux_pkg {

void Aux::on_tick() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[aux] tick=%d\n", count_);
    }
    count_++;
}

::rclcpp::Result Aux::configure(::rclcpp::Node& node) {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/aux");
    if (!r.ok()) return r;
    return node.create_wall_timer<Aux, &Aux::on_tick>(timer_, 50, this);
}

} // namespace aux_pkg
