// SPDX-License-Identifier: Apache-2.0
//
// The program a stock ament package writes: init, construct, spin.
#include <memory>

#include "rclcpp/rclcpp.hpp"
#include "workspace_port_nodes/counter.hpp"

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<workspace_port_nodes::Counter>());
    rclcpp::shutdown();
    return 0;
}
