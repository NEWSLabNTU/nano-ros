// SPDX-License-Identifier: Apache-2.0
#include "workspace_port_nodes/counter.hpp"

#include <chrono>

// The library's own definition must reach its sources wherever they compile.
#ifndef WORKSPACE_PORT_BUILDING_LIBRARY
#error "target_compile_definitions(counter_component ...) did not reach counter.cpp"
#endif

using namespace std::chrono_literals;

namespace workspace_port_nodes {

Counter::Counter() : rclcpp::Node("workspace_port"), topic_("workspace_port_topic") {
    pub_ = create_publisher<std_msgs::msg::Int32>(topic_, 10);
    timer_ = create_wall_timer(200ms, [this]() { on_timer(); });
    RCLCPP_INFO(get_logger(), "workspace_port up; publishing std_msgs/Int32 on %s",
                topic_.c_str());
}

void Counter::on_timer() {
    std_msgs::msg::Int32 msg;
    msg.data = count_++;
    pub_->publish(msg);
}

} // namespace workspace_port_nodes
