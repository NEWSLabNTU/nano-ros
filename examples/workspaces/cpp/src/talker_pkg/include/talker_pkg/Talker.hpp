#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace talker_pkg {

/// Talker — the ROS 2 tutorial talker (demo_nodes_cpp `talker.cpp`) as a
/// stateful component (RFC-0043). `configure` creates a publisher on
/// `/chatter` and binds the member `on_tick` (by identity, no name) as a 1 Hz
/// timer callback that publishes `std_msgs/String` "Hello World: N", N counting
/// from 1, and prints `Publishing: '<payload>'`. phase-535 made this the ONE
/// C++ talker: the standalone example pairs and the workspace images run the
/// same program (RFC-0122 D1, option B). The generated entry constructs this
/// object in static storage and calls `configure(node)`; the executor
/// dispatches `on_tick` during `spin_once`.
class Talker {
    ::rclcpp::Publisher<std_msgs::msg::String> pub_;
    ::rclcpp::Timer timer_;
    // The message the timer fills: its capacity is the workspace's
    // `std_msgs/String` bound (`nros-codegen.toml`).
    std_msgs::msg::String msg_;
    int count_ = 0;

    void on_tick(); // real body; bound via &Talker::on_tick (no callback name)

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace talker_pkg
