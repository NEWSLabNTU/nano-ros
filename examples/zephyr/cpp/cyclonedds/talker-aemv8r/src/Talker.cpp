/// @file Talker.cpp
/// @brief FVP AEMv8-R Cortex-A/R cyclonedds talker — typed component
///        (RFC-0043 / phase-244.C2.1). Publishes std_msgs/Int32 on /chatter at
///        1 Hz; pair with a stock ROS 2 `ros2 topic echo /chatter` peer.

#include "Talker.hpp"

#include <cstdio>

namespace nros_zephyr_aemv8r_cyclonedds_talker {

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
    return ::nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
}

} // namespace nros_zephyr_aemv8r_cyclonedds_talker
