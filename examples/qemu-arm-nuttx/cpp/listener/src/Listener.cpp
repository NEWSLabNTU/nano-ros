/// @file Listener.cpp
/// @brief NuttX C++ listener — typed component (RFC-0043, phase-240.3).

#include "Listener.hpp"

#include <cstdio>

namespace nuttx_cpp_listener {

void Listener::on_raw(const uint8_t* data, size_t len) {
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

::nros::Result Listener::configure(::nros::Node& node) {
    // Raw (zero-copy) subscription bound to the member by identity. The type
    // name is passed verbatim to the wire keyexpr — kept as the ROS slash form
    // to match the sibling `nuttx_cpp_talker` (still declarative). RFC-0043's
    // typed `Publisher<Int32>` registers the DDS-mangled form; raw↔typed
    // type-name unification is tracked separately (240.1 finding / 240.2b).
    ::nros::Result r = ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", "std_msgs::msg::dds_::Int32_", this);
    if (r.ok()) {
        // Readiness marker the rtos_e2e harness greps before driving the talker.
        std::printf("Waiting for messages\n");
    }
    return r;
}

} // namespace nuttx_cpp_listener
