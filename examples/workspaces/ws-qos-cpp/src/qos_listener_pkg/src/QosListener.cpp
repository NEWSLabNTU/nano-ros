// QosListener — typed component (RFC-0043), the C++ projection of ws-qos-c's
// QosListener. `configure` builds the SAME non-default `nros::QoS`
// (`.reliable().transient_local().keep_last(10)`) the talker declares and passes
// it to the raw subscription bind. Matching the per-entity QoS contract is what
// lets the QoS-tagged endpoints connect; a mismatch makes the listener receive
// nothing.

#include "qos_listener_pkg/QosListener.hpp"

#include <cstdio>

namespace qos_listener_pkg {

void QosListener::on_raw(const uint8_t* data, size_t len) {
    // CDR-encoded std_msgs/Int32: 4-byte encapsulation header, then the LE i32.
    int32_t v = 0;
    if (len >= 8) {
        v = static_cast<int32_t>(
            static_cast<uint32_t>(data[4]) | (static_cast<uint32_t>(data[5]) << 8) |
            (static_cast<uint32_t>(data[6]) << 16) | (static_cast<uint32_t>(data[7]) << 24));
    }
    std::printf("Received: %d\n", v);
    ++recv_;
}

::nros::Result QosListener::configure(::nros::Node& node) {
    // `::setvbuf` (C global): line-buffer stdout so each `Received:` flushes live.
    ::setvbuf(stdout, nullptr, _IOLBF, 0);
    // Byte-identical to the talker's profile — both endpoints must declare the same
    // RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10) contract to connect.
    const ::nros::QoS qos =
        ::nros::QoS::default_profile().reliable().transient_local().keep_last(10);
    // The typed `Publisher<Int32>` registers the DDS-mangled keyexpr, so the raw
    // sub must match on `Int32::TYPE_NAME` (240.1 finding).
    return ::nros::bind_subscription_raw<QosListener, &QosListener::on_raw>(
        node, "/chatter", std_msgs::msg::Int32::TYPE_NAME, this, qos);
}

} // namespace qos_listener_pkg
