// SafetyTalker — counter publisher on /chatter (std_msgs/Int32).
// Message integrity is not declared here: the bringup's
// [system].features = ["safety"] turns it on for the whole system, and the
// runtime attaches the per-sample CRC on publish. See demo_bringup/system.toml.

#include "cpp_safety_talker_pkg/SafetyTalker.hpp"

#include <cstdio>

namespace cpp_safety_talker_pkg {

void SafetyTalker::on_tick() {
    std_msgs::msg::Int32 m;
    m.data = counter_++;
    if (pub_.publish(m).ok()) {
        std::printf("[TALKER] Published: %d\n", m.data);
        std::fflush(stdout);
    }
}

::rclcpp::Result SafetyTalker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<SafetyTalker, &SafetyTalker::on_tick>(timer_, 1000, this);
}

} // namespace cpp_safety_talker_pkg
