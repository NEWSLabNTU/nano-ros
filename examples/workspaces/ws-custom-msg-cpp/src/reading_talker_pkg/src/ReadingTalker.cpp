// ReadingTalker — raw-CDR typed component (RFC-0043), the C++ projection of
// ws-custom-msg-c's ReadingTalker. The workspace-local `custom_msgs/Reading`
// schema is YOURS (src/custom_msgs/msg/Reading.msg); the publisher carries the
// type name as a string and hand-encodes the CDR payload — no generated bindings.

#include "reading_talker_pkg/ReadingTalker.hpp"

#include <cstdio>
#include <cstring>

namespace reading_talker_pkg {

// CDR encode of custom_msgs/Reading:
//   [0..4)   encapsulation header (CDR_LE)
//   [4..12)  float64 temperature (8-aligned: stream pos 0)
//   [12..20) float64 humidity    (8-aligned: stream pos 8)
//   [20..24) int32   sequence    (4-aligned: stream pos 16)
// Total 24 payload bytes; the host is little-endian (x86_64) so the native
// double/int byte order already matches CDR_LE — a plain memcpy suffices.
void ReadingTalker::on_tick() {
    double temperature = 20.0 + static_cast<double>(count_) * 0.5;
    double humidity = 50.0;
    int32_t sequence = count_;

    uint8_t buf[24];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    std::memcpy(buf + 4, &temperature, 8);
    std::memcpy(buf + 12, &humidity, 8);
    std::memcpy(buf + 20, &sequence, 4);

    if (pub_.publish_raw(buf, sizeof(buf)).ok()) {
        std::printf("[reading_talker] sent seq=%d temp=%.1f\n", static_cast<int>(sequence),
                    temperature);
    }
    count_++;
}

::nros::Result ReadingTalker::configure(::nros::Node& node) {
    // `::setvbuf` (C global): line-buffer stdout so each `sent seq=` flushes live.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/reading");
    if (!r.ok()) return r;
    return ::nros::bind_timer<ReadingTalker, &ReadingTalker::on_tick>(node, timer_, 1000, this);
}

} // namespace reading_talker_pkg
