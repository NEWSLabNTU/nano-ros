// QEMU RISC-V ThreadX C++ talker — typed component (RFC-0043). A timer member
// publishes a the official ROS 2 demo payload (`std_msgs/String`, `Hello World: N`)
// on `/chatter` via a typed `Publisher<String>`.
// No string callback name, no synthesizing interpreter — the executor dispatches
// `on_tick` each spin tick. Platform / RMW (zenoh|cyclonedds) selection lives in
// CMake, not here.
#ifndef RISCV64_THREADX_CPP_TALKER_TALKER_HPP
#define RISCV64_THREADX_CPP_TALKER_TALKER_HPP

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace riscv64_threadx_cpp_talker {

class Talker {
    ::nros::Publisher<std_msgs::msg::String> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace riscv64_threadx_cpp_talker

#endif // RISCV64_THREADX_CPP_TALKER_TALKER_HPP
