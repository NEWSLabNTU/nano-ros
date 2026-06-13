// ThreadX-Linux C++ Fibonacci action server — typed component (RFC-0043).
// `configure` binds the member goal/cancel callbacks (by identity) as a raw action
// server on `/fibonacci` + a timer that drives goal execution: decode the CDR goal
// (int32 order), compute the sequence, complete with a CDR result. No interpreter.
#ifndef THREADX_LINUX_CPP_ACTION_SERVER_ACTIONSERVER_HPP
#define THREADX_LINUX_CPP_ACTION_SERVER_ACTIONSERVER_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace threadx_linux_cpp_action_server {

class ActionServer {
    ::nros::ActionServerStorage storage_;
    ::nros::Timer timer_;
    void* executor_ = nullptr;

    bool pending_ = false;
    uint8_t goal_id_[16] = {};
    int32_t order_ = 0;

    int32_t on_goal(const uint8_t goal_id[16], const uint8_t* data, size_t len);
    int32_t on_cancel(const uint8_t goal_id[16]);
    void on_tick();

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace threadx_linux_cpp_action_server

#endif
