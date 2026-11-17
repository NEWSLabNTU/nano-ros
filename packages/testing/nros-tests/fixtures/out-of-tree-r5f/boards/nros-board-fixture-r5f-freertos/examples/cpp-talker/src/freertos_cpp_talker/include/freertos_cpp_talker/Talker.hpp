// The ROS 2 tutorial talker (demo_nodes_cpp `talker.cpp`) as a typed component
// (RFC-0043): a 1 Hz timer publishes `std_msgs/String` "Hello World: N" on
// `/chatter`, N counting from 1, and prints `Publishing: '<payload>'`.

#ifndef FREERTOS_CPP_TALKER_TALKER_HPP
#define FREERTOS_CPP_TALKER_TALKER_HPP

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace freertos_cpp_talker {

class Talker {
    ::rclcpp::Publisher<std_msgs::msg::String> pub_;
    ::rclcpp::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body, bound by identity

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace freertos_cpp_talker

#endif // FREERTOS_CPP_TALKER_TALKER_HPP
