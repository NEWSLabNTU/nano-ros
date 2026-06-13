/// @file Listener.cpp
/// @brief QEMU RISC-V ThreadX C++ listener — typed component (RFC-0043). Real `on_raw`
///        body bound by identity (raw zero-copy sub); no callback name, no interpreter.

#include "Listener.hpp"

#include <cstdio>

namespace riscv64_threadx_cpp_listener {

void Listener::on_raw(const uint8_t* data, size_t len) {
    // CDR-encoded std_msgs/Int32: 4-byte encapsulation header, then the LE i32.
    int32_t v = 0;
    if (len >= 8) {
        v = static_cast<int32_t>(
            static_cast<uint32_t>(data[4]) | (static_cast<uint32_t>(data[5]) << 8) |
            (static_cast<uint32_t>(data[6]) << 16) | (static_cast<uint32_t>(data[7]) << 24));
    }
    printf("Received: %d\n", v);
    ++recv_;
}

::nros::Result Listener::configure(::nros::Node& node) {
    setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r = ::nros::bind_subscription_raw<Listener, &Listener::on_raw>(
        node, "/chatter", "std_msgs::msg::dds_::Int32_", this);
    if (r.ok()) {
        // Readiness marker the rtos_e2e harness greps before driving the talker.
        printf("Waiting for messages\n");
    }
    return r;
}

} // namespace riscv64_threadx_cpp_listener
