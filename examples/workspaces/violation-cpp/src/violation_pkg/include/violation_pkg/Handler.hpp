#pragma once

#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace violation_pkg {

/// phase-474 T4 - an INIT/RUN handler in the safety island's shape: start-up
/// work in INIT (suppressed: the monitors are not armed yet), arming on RUN,
/// then one deliberate overrun of its contracted 50 ms path.
class Handler : public ::nros::NodeWithTimers<1> {
    ::rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    int tick_ = 0;

    void on_timer();

  public:
    explicit Handler(::nros::NodeHandle h);
};

} // namespace violation_pkg
