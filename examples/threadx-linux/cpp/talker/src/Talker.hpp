// ThreadX-Linux C++ talker — typed component (RFC-0043). A timer member
// publishes a real Int32 counter on `/chatter` via a typed `Publisher<Int32>`.
// No string callback name, no synthesizing interpreter — the executor dispatches
// `on_tick` each spin tick. Platform/RMW selection lives in CMake, not here.
#ifndef THREADX_LINUX_CPP_TALKER_TALKER_HPP
#define THREADX_LINUX_CPP_TALKER_TALKER_HPP

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

namespace threadx_linux_cpp_talker {

class Talker {
    ::nros::Publisher<std_msgs::msg::Int32> pub_;
    ::nros::Timer timer_;
    int count_ = 0;

    void on_tick(); // real body, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_talker

#endif // THREADX_LINUX_CPP_TALKER_TALKER_HPP
