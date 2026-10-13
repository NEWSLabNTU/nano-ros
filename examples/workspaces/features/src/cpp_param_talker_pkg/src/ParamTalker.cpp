// ParamTalker — Phase 269 W1: typed C++ component that reads `publish_period_ms`
// LIVE from the executor-backed parameter store on each tick, publishes on /chatter.
// The launch-baked initial is 250; `ros2 param set publish_period_ms N` changes the
// published value live, proving the C++ in-callback live read path.

#include "cpp_param_talker_pkg/ParamTalker.hpp"

#include <cstdio>

namespace cpp_param_talker_pkg {

void ParamTalker::on_tick() {
    // Phase 269 W1 — live param read: re-read publish_period_ms from the node's
    // parameter store each tick (`ros2 param set` writes the same store). Boots at
    // the launch-baked initial (250).
    int64_t live = -1;
    (void)node_->get_parameter<int64_t>("publish_period_ms", live);

    std_msgs::msg::Int32 m;
    m.data = static_cast<int32_t>(live);
    if (pub_.publish(m).ok()) {
        std::printf("Published: %d\n", m.data);
    }
}

::rclcpp::Result ParamTalker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    node_ = &node;
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<ParamTalker, &ParamTalker::on_tick>(timer_, 500, this);
}

} // namespace cpp_param_talker_pkg
