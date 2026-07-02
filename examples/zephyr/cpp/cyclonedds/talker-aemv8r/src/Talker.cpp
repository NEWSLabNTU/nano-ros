/// @file Talker.cpp
/// @brief FVP AEMv8-R Cortex-A/R cyclonedds talker — typed component
///        (RFC-0043 / phase-244.C2.1). Publishes the official ROS 2 demo
///        payload (std_msgs/String, "Hello World: N") on /chatter at 1 Hz;
///        pair with a stock ROS 2 `ros2 topic echo /chatter` peer.

#include "Talker.hpp"

#include <cstdio>

namespace nros_zephyr_aemv8r_cyclonedds_talker {

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
    ::nros::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
}

} // namespace nros_zephyr_aemv8r_cyclonedds_talker
