// Aux.cpp — realtime-cpp mid-tier auxiliary node.
//
// Publishes a monotonic Int32 counter on /aux every 50 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization). Runs on the `mid`
// tier (RFC-0015 Model 1, one tier task per tier). Which tier is the bringup's
// call, not this node's — `demo_bringup/system.toml` carries
// `group_tiers = { aux = "mid" }`, and the `[tiers.mid.*]` blocks beside it say
// what `mid` means on each platform. The mid tier is spawned by a spawned tier
// (boot→mid→low) — a `[aux] tick` proves the #144 chained spawn serialized the
// declares so this tier's publisher write filter opened.

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
