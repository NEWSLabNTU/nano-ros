/// @file Listener.cpp
/// @brief ThreadX-Linux C++ listener — typed component (RFC-0043). Real `on_raw`
///        body bound by identity (raw zero-copy sub); no callback name, no interpreter.

#include "Listener.hpp"

#include <cstdio>

namespace threadx_linux_cpp_listener {

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
    setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r = ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", std_msgs::msg::String::TYPE_NAME, this);
    if (r.ok()) {
        // Readiness marker the rtos_e2e harness greps before driving the talker.
        printf("Waiting for messages\n");
    }
    return r;
}

} // namespace threadx_linux_cpp_listener
