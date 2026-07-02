// Zephyr C++ talker — typed component (RFC-0043 / phase-240.8). A timer member
// publishes the official ROS 2 demo payload (`std_msgs/String`, `Hello World: N`)
// on `/chatter` via a typed `Publisher<String>`.
// The Zephyr typed carrier (`zephyr_entry_main_typed.cpp.in`) constructs this
// object + calls `configure(node)` and runs `ZephyrBoard::run_components`.
#ifndef NROS_ZEPHYR_TALKER_TYPED_TALKER_HPP
#define NROS_ZEPHYR_TALKER_TYPED_TALKER_HPP

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace nros_zephyr_talker_typed_cpp {

class Talker {
    ::nros::Publisher<std_msgs::msg::String> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace nros_zephyr_talker_typed_cpp

#endif // NROS_ZEPHYR_TALKER_TYPED_TALKER_HPP
