// Ctrl.cpp — ws-realtime-cpp-rclcpp Phase 272 W3 high-tier control node.
//
// rclcpp-shape (IS-A-node): the ctor receives the executor-bound NodeHandle,
// constructs the base ComponentNode with name "ctrl_node" (matching system.toml),
// and wires the publisher + 10 ms timer entirely in the ctor body — no separate
// `configure(Node&)` step. The tier binding resolves via the W2-seeded
// `node_name → sched_context` table at the `node_builder("ctrl_node")` call inside
// ComponentNode's base ctor, which goes through nros_cpp_node_create → node_builder.
//
// Publishes a monotonic Int32 counter on /ctrl every 10 ms (same cadence as the
// configure-shape ws-realtime-cpp). The e2e asserts ctrl_n >= telem_n * 3 (≥3× rate).

#include "ctrl_pkg/Ctrl.hpp"

#include <cstdio>
#include <cstring>

namespace ctrl_pkg {

// CDR encode of std_msgs/Int32: 4-byte encap header (CDR_LE) + 4-byte int32.
void Ctrl::on_tick() {
    int32_t data = static_cast<int32_t>(count_);
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &data, 4);
    if (pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[rclcpp_ctrl] tick=%d\n", count_);
    }
    count_++;
}

Ctrl::Ctrl(::nros::NodeHandle h)
    : ::nros::ComponentNode(h, "ctrl_node") {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // create_publisher / create_timer forward to the owned node_ (bound to the
    // high-tier sched context via the W2-seeded table — no explicit sched call needed).
    pub_ = create_publisher<Int32Tag>("/ctrl");
    NROS_CREATE_TIMER(10, on_tick);
}

} // namespace ctrl_pkg
