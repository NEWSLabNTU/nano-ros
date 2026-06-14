/// @file Listener.cpp
/// @brief multi-package-workspace demo — C++ listener, typed component
/// (RFC-0043 / phase-244.C4).

#include "Listener.hpp"

#include <cstdio>

#include "std_msgs.hpp"

namespace pkg_cpp_listener {

void Listener::on_raw(const uint8_t* data, size_t len) {
    // CDR-encoded std_msgs/Int32: 4-byte encapsulation header, then the LE i32.
    int32_t v = 0;
    if (len >= 8) {
        v = static_cast<int32_t>(
            static_cast<uint32_t>(data[4]) | (static_cast<uint32_t>(data[5]) << 8) |
            (static_cast<uint32_t>(data[6]) << 16) | (static_cast<uint32_t>(data[7]) << 24));
    }
    std::printf("[pkg_cpp_listener] received: %d (total=%d)\n", v, ++recv_);
}

::nros::Result Listener::configure(::nros::Node& node) {
    // Raw (zero-copy) subscription bound to the member by identity; type name is
    // the DDS-mangled form the sibling typed `Publisher<Int32>` registers.
    ::nros::Result r = ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", std_msgs::msg::Int32::TYPE_NAME, this);
    if (r.ok()) {
        std::printf("[pkg_cpp_listener] subscribed to /chatter\n");
    }
    return r;
}

} // namespace pkg_cpp_listener
