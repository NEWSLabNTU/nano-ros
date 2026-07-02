// Telem.cpp — ws-realtime-cpp-mps2 low-tier telemetry node.
//
// Publishes a monotonic Int32 counter on /telem every 100 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization). Runs on the
// low-priority FreeRTOS tier task (priority 2) via FreertosBoard::run_tiers
// (RFC-0015 Model 1 embedded).

#include "telem_pkg/Telem.hpp"

#include <cstdio>

namespace telem_pkg {

void Telem::on_tick() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[telem] tick=%d\n", count_);
    }
    count_++;
}

::nros::Result Telem::configure(::nros::Node& node) {
    ::nros::Result r = node.create_publisher(pub_, "/telem");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Telem, &Telem::on_tick>(node, timer_, 100, this);
}

} // namespace telem_pkg
