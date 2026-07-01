// Telem.cpp — ws-realtime-cpp-rclcpp Phase 272 W3 low-tier telemetry node.
//
// rclcpp-shape (IS-A-node): ctor creates publisher + 100 ms timer. Tier binding
// resolved via the W2-seeded `node_name → sched_context` table (telem_node → low tier).
// Publishes a monotonic Int32 counter on /telem every 100 ms; the e2e asserts
// ctrl_n >= telem_n * 3 (the high-tier /ctrl at 10 ms publishes ≥3× as many).

#include "telem_pkg/Telem.hpp"

#include <cstdio>
#include <cstring>

namespace telem_pkg {

// CDR encode of std_msgs/Int32: 4-byte encap header (CDR_LE) + 4-byte int32.
void Telem::on_tick() {
    int32_t data = static_cast<int32_t>(count_);
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &data, 4);
    if (pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[rclcpp_telem] tick=%d\n", count_);
    }
    count_++;
}

Telem::Telem(::nros::NodeHandle h)
    : ::nros::ComponentNode(h, "telem_node") {
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    pub_ = create_publisher<Int32Tag>("/telem");
    NROS_CREATE_TIMER(100, on_tick);
}

} // namespace telem_pkg
