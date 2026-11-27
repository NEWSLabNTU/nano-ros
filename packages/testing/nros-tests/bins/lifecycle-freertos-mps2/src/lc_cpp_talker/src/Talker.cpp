// A managed C++ talker on FreeRTOS + MPS2-AN385. The bringup's
// `[lifecycle] autostart = "none"` leaves it `unconfigured` at boot, so it must
// publish nothing until a client configures and activates it.

#include "lc_cpp_talker/Talker.hpp"

#include <cstdio>

namespace lc_cpp_talker {

void Talker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = counter_++;
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
    }
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

} // namespace lc_cpp_talker
