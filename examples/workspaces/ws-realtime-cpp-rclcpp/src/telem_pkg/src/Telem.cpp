// Telem.cpp — ws-realtime-cpp-rclcpp low-tier telemetry node.
//
// rclcpp-shape (IS-A-node): ctor creates publisher + 100 ms timer. Tier binding
// resolved via the seeded `node_name → sched_context` table (telem_node → low tier).
// Publishes a monotonic Int32 counter on /telem every 100 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization); the e2e asserts
// ctrl_n >= telem_n * 3 (the high-tier /ctrl at 10 ms publishes ≥3× as many).

#include "telem_pkg/Telem.hpp"

#include <cstdio>

namespace telem_pkg {

void Telem::on_tick() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[rclcpp_telem] tick=%d\n", count_);
    }
    count_++;
}

Telem::Telem(::nros::NodeHandle h) : ::nros::ComponentNode(h, "telem_node") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher<std_msgs::msg::Int32>("/telem");
    NROS_CREATE_TIMER(100, on_tick);
}

} // namespace telem_pkg
