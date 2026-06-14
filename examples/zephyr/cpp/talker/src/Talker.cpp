/// @file Talker.cpp
/// @brief Zephyr C++ talker — typed component (RFC-0043 / phase-244.C2).

#include "Talker.hpp"

#include <cstdio>

namespace nros_zephyr_talker_cpp {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = count_++;
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
    }
}

::nros::Result Talker::configure(::nros::Node& node) {
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 500, this);
}

} // namespace nros_zephyr_talker_cpp
