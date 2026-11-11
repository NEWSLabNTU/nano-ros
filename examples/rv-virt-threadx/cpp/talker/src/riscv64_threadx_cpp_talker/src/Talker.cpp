// See Talker.hpp. The generated entry runs `rclcpp::init`, creates the node
// and spins; this file holds only the node.

#include "riscv64_threadx_cpp_talker/Talker.hpp"

#include <cstdio>

namespace riscv64_threadx_cpp_talker {

void Talker::on_tick() {
    // Pre-increment so the first payload is "Hello World: 1", as the ROS 2 demo talker does.
    ++count_;
    char payload[64];
    std::snprintf(payload, sizeof(payload), "Hello World: %d", count_);
    std_msgs::msg::String m;
    m.data = payload;
    if (pub_.publish(m).ok()) {
        std::printf("Publishing: '%s'\n", m.data.c_str());
    }
}

::rclcpp::Result Talker::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global): a test reads this output live through a pipe.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
}

} // namespace riscv64_threadx_cpp_talker
