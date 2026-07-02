/// @file Listener.cpp
/// @brief FreeRTOS C++ listener — typed component (RFC-0043, phase-240.6).

#include "Listener.hpp"

#include <cstdio>

namespace freertos_cpp_listener {

void Listener::on_raw(const uint8_t* data, size_t len) {
    // Decode via the generated typed deserializer (phase-277 W4; was
    // hand-rolled CDR) and log the official ROS 2 demo line.
    std_msgs::msg::String m;
    if (std_msgs::msg::String::ffi_deserialize(data, len, &m) == 0) {
        std::printf("I heard: [%s]\n", m.data.c_str());
        ++recv_;
    }
}

::nros::Result Listener::configure(::nros::Node& node) {
    // Raw (zero-copy) subscription bound to the member by identity. The type
    // name is passed verbatim to the wire keyexpr — kept as the ROS slash form
    // to match the sibling `freertos_cpp_talker` (still declarative). RFC-0043's
    // typed `Publisher<String>` registers the DDS-mangled form; raw↔typed
    // type-name unification is tracked separately (240.1 finding / 240.2b).
    ::nros::Result r = ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", std_msgs::msg::String::TYPE_NAME, this);
    if (r.ok()) {
        // Readiness marker the rtos_e2e harness greps before driving the talker.
        std::printf("Waiting for messages\n");
    }
    return r;
}

} // namespace freertos_cpp_listener
