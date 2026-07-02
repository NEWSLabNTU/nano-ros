// Ctrl.cpp — ws-realtime-cpp-mps2 high-tier control node.
//
// Publishes a monotonic Int32 counter on /ctrl every 10 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization). Runs on the
// high-priority FreeRTOS tier task (priority 5) via FreertosBoard::run_tiers
// (RFC-0015 Model 1 embedded).

#include "ctrl_pkg/Ctrl.hpp"

#include <cstdio>

namespace ctrl_pkg {

void Ctrl::on_tick() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[ctrl] tick=%d\n", count_);
    }
    count_++;
}

::nros::Result Ctrl::configure(::nros::Node& node) {
    ::nros::Result r = node.create_publisher(pub_, "/ctrl");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Ctrl, &Ctrl::on_tick>(node, timer_, 10, this);
}

} // namespace ctrl_pkg
