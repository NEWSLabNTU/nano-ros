// Telem.cpp — ws-realtime-cpp Phase 269 W4 low-tier telemetry node.
//
// Publishes a monotonic Int32 counter on /telem every 100 ms using the raw-CDR
// path (RFC-0043). Running at 1/10 the cadence of ctrl_pkg, the e2e test
// asserts ctrl publishes at least 3× as many messages as telem in the same window.

#include "telem_pkg/Telem.hpp"

#include <cstdio>
#include <cstring>

namespace telem_pkg {

// CDR encode of std_msgs/Int32: 4-byte encap header + 4-byte int32 = 8 bytes.
void Telem::on_tick() {
    int32_t data = static_cast<int32_t>(count_);
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &data, 4);
    if (pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[telem] tick=%d\n", count_);
    }
    count_++;
}

::nros::Result Telem::configure(::nros::Node &node) {
    // Line-buffer stdout so each tick flushes immediately when piped.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/telem");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Telem, &Telem::on_tick>(node, timer_, 100, this);
}

} // namespace telem_pkg
