/// @file Talker.cpp
/// @brief ThreadX-Linux C++ talker — typed component (RFC-0043). Real `on_tick`
///        body bound by identity; no string callback name, no interpreter.

#include "Talker.hpp"

#include <cstdio>

namespace threadx_linux_cpp_talker {

void Talker::on_tick() {
    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker.
    ++count_;
    char payload[64];
    std::snprintf(payload, sizeof(payload), "Hello World: %d", count_);
    std_msgs::msg::String m;
    m.data = payload;
    if (pub_.publish(m).ok()) {
        std::printf("Publishing: '%s'\n", m.data.c_str());
    }
}

::nros::Result Talker::configure(::nros::Node& node) {
    std::setvbuf(stdout, nullptr, _IONBF, 0);
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    // Member-fn-pointer-as-template-param → no-alloc trampoline; `this` is ctx.
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 500, this);
}

} // namespace threadx_linux_cpp_talker
