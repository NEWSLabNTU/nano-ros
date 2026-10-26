// SPDX-License-Identifier: Apache-2.0
#ifndef WORKSPACE_PORT_NODES__COUNTER_HPP_
#define WORKSPACE_PORT_NODES__COUNTER_HPP_

#include <cstdint>
#include <string>

#include "rclcpp/rclcpp.hpp"
#include "std_msgs/msg/int32.hpp"
#include "workspace_port_core/sequence.hpp"

namespace workspace_port_nodes {

/// Publishes a counter on `workspace_port_topic` every 200 ms.
class Counter : public rclcpp::Node {
  public:
    Counter();

  private:
    void on_timer();

    std::string topic_;
    rclcpp::Publisher<std_msgs::msg::Int32>::SharedPtr pub_;
    rclcpp::TimerBase::SharedPtr timer_;
    workspace_port_core::Sequence sequence_;
};

} // namespace workspace_port_nodes

#endif // WORKSPACE_PORT_NODES__COUNTER_HPP_
