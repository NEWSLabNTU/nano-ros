/// @file Talker.cpp
/// @brief NuttX C++ talker — typed component (RFC-0043).

#include "Talker.hpp"

#include <cstdio>

namespace nuttx_cpp_talker {

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
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 500, this);
}

} // namespace nuttx_cpp_talker
