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

// Issue 2638 -- the list parameters `config/cpp_params.param.yaml` loads
// through `<param from>` must arrive as arrays of their element type. They
// used to arrive as one comma-joined string, which no array read accepts.
static bool list_params_arrived_as_arrays(const ::rclcpp::Node& node) {
    ::rclcpp::Seq<double, 8> gains;
    ::rclcpp::Seq<int64_t, 8> ids;
    ::rclcpp::Seq<bool, 8> flags;
    bool ok = node.get_parameter("gains", gains) && gains.size() == 3 && gains[0] == 40.0 &&
              gains[1] == 50.0 && gains[2] == 60.0;
    ok = ok && node.get_parameter("ids", ids) && ids.size() == 3 && ids[0] == 1 && ids[2] == 3;
    ok = ok && node.get_parameter("flags", flags) && flags.size() == 2 && flags[0] && !flags[1];
    // A string array has no C++ read; its TYPE is what was wrong.
    ok = ok && node.get_parameter_type("names") == ::rclcpp::PARAMETER_STRING_ARRAY;
    // `[]` has no value in ROS, so nothing was seeded.
    ok = ok && node.get_parameter_type("unset_list") == ::rclcpp::PARAMETER_NOT_SET;
    return ok;
}

::rclcpp::Result ParamTalker::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    node_ = &node;
    std::printf("list params: %s\n", list_params_arrived_as_arrays(node) ? "arrays" : "WRONG");
    ::rclcpp::Result r = node.create_publisher(pub_, "/chatter");
    if (!r.ok()) return r;
    return node.create_wall_timer<ParamTalker, &ParamTalker::on_tick>(timer_, 500, this);
}

} // namespace cpp_param_talker_pkg
