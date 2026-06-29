// ReadingListener — raw-CDR typed component (RFC-0043), the C++ projection of
// ws-custom-msg-c's ReadingListener. Carries the workspace-local type name as a
// string and hand-decodes the CDR `Reading` payload — no generated bindings.

#include "reading_listener_pkg/ReadingListener.hpp"

#include <cstdio>
#include <cstring>

namespace reading_listener_pkg {

// CDR-encoded custom_msgs/Reading (see ReadingTalker.cpp for the layout):
//   [4..12)  float64 temperature
//   [12..20) float64 humidity
//   [20..24) int32   sequence
// Host is little-endian (x86_64), so memcpy back into native types matches.
void ReadingListener::on_raw(const uint8_t* data, size_t len) {
    if (len >= 24) {
        double temperature;
        int32_t sequence;
        std::memcpy(&temperature, data + 4, 8);
        std::memcpy(&sequence, data + 20, 4);
        std::printf("reading seq=%d temp=%.1f\n", static_cast<int>(sequence), temperature);
        ++recv_;
    }
}

::nros::Result ReadingListener::configure(::nros::Node& node) {
    // `::setvbuf` (C global): line-buffer stdout so each `reading seq=` flushes live.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // Match the talker's raw type-name string exactly (the keyexpr the raw
    // publisher registers); no generated header needed.
    return ::nros::bind_subscription_raw<ReadingListener, &ReadingListener::on_raw>(
        node, "/reading", "custom_msgs::msg::dds_::Reading_", this);
}

} // namespace reading_listener_pkg
