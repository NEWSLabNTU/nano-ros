/// @file Talker.cpp
/// @brief ThreadX-Linux C++ talker — typed component (RFC-0043). Real `on_tick`
///        body bound by identity; no string callback name, no interpreter.

#include "Talker.hpp"

#include <cstdio>

namespace threadx_linux_cpp_talker {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_++;
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
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
