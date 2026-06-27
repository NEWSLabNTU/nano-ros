// Listener — typed component (RFC-0043). Real `on_raw` body bound by identity.

#include "cpp_listener_pkg/Listener.hpp"

#include <cstdio>

namespace cpp_listener_pkg {

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
    // `::setvbuf`, not `std::setvbuf` — Zephyr picolibc declares it only at global scope
    // (phase-263 C2c-zephyr; same fix as the cpp workspace listener).
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    // The typed `Publisher<Int32>` registers the DDS-mangled keyexpr, so the
    // raw sub must match on `Int32::TYPE_NAME` (240.1 finding).
    return ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", std_msgs::msg::Int32::TYPE_NAME, this);
}

} // namespace cpp_listener_pkg
