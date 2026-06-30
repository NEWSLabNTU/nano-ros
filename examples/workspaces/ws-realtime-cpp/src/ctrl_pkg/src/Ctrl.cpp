// Ctrl.cpp — ws-realtime-cpp Phase 269 W4 high-tier control node.
//
// Publishes a monotonic Int32 counter on /ctrl every 10 ms using the raw-CDR
// path (RFC-0043): the publisher carries TYPE_NAME as a string and the payload
// is hand-encoded — no generated message binding needed. The configure-shape
// receives a Node& that is already bound to the high-priority sched context
// (emitted by the W4 C++ codegen path via NodeBuilder::sched).

#include "ctrl_pkg/Ctrl.hpp"

#include <cstdio>
#include <cstring>

namespace ctrl_pkg {

// CDR encode of std_msgs/Int32:
//   [0..4)  encapsulation header (CDR_LE)
//   [4..8)  int32 data
// Total 8 bytes; the host is little-endian — plain memcpy suffices.
void Ctrl::on_tick() {
    int32_t data = static_cast<int32_t>(count_);
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &data, 4);
    if (pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[ctrl] tick=%d\n", count_);
    }
    count_++;
}

::nros::Result Ctrl::configure(::nros::Node &node) {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/ctrl");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Ctrl, &Ctrl::on_tick>(node, timer_, 10, this);
}

} // namespace ctrl_pkg
