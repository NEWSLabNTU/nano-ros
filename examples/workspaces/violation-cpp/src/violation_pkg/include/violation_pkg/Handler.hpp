#pragma once

#include <rclcpp/rclcpp.hpp>

#include "std_msgs.hpp"

namespace violation_pkg {

/// phase-474 T4 - an INIT/RUN handler in the safety island's shape: start-up
/// work in INIT (suppressed: the monitors are not armed yet), arming on RUN,
/// then one deliberate overrun of its contracted 50 ms path. phase10-B3: and
/// a route, from its own take of /t4/avail to /t4/answer, made late once.
class Handler : public ::rclcpp::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    ::rclcpp::Publisher<std_msgs::msg::Int32> answer_;
    ::rclcpp::Publisher<std_msgs::msg::Int32> avail_;
    int tick_ = 0;

    void on_timer();

  public:
    void on_avail(const std_msgs::msg::Int32& msg);

  private:

  public:
    explicit Handler(::rclcpp::NodeHandle h);
};

} // namespace violation_pkg
