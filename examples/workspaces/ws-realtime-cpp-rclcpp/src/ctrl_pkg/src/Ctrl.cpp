// Ctrl.cpp — ws-realtime-cpp-rclcpp high-tier control node.
//
// rclcpp-shape (IS-A-node): the ctor receives the executor-bound NodeHandle,
// constructs the base ComponentNode with name "ctrl_node" (matching system.toml),
// and wires the publisher + 10 ms timer entirely in the ctor body — no separate
// `configure(Node&)` step. The tier binding resolves via the seeded
// `node_name → sched_context` table at the `node_builder("ctrl_node")` call inside
// ComponentNode's base ctor, which goes through nros_cpp_node_create → node_builder.
//
// Publishes a monotonic Int32 counter on /ctrl every 10 ms via the typed
// Publisher<std_msgs::msg::Int32> (generated serialization, same cadence as the
// configure-shape ws-realtime-cpp). The e2e asserts ctrl_n >= telem_n * 3 (≥3× rate).

#include "ctrl_pkg/Ctrl.hpp"

#include <cstdio>

namespace ctrl_pkg {

void Ctrl::on_tick() {
    std_msgs::msg::Int32 msg;
    msg.data = count_;
    if (pub_.publish(msg).ok()) {
        std::printf("[rclcpp_ctrl] tick=%d\n", count_);
    }
    count_++;
}

Ctrl::Ctrl(::nros::NodeHandle h) : ::nros::ComponentNode(h, "ctrl_node") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // create_publisher / create_timer forward to the owned node_ (bound to the
    // high-tier sched context via the seeded table — no explicit sched call needed).
    pub_ = create_publisher<std_msgs::msg::Int32>("/ctrl");
    NROS_CREATE_TIMER(10, on_tick);
}

} // namespace ctrl_pkg
