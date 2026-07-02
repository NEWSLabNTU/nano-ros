// QEMU RISC-V ThreadX C++ Fibonacci action client — typed CALLBACK component (RFC-0041/0043).
// `configure` binds member callbacks (goal-response / feedback / result) by identity
// via `bind_action_client`, then sends one goal; the acceptance + result arrive in
// the member callbacks, pumped by the binding's poll-timer each spin tick.
#ifndef RISCV64_THREADX_CPP_ACTION_CLIENT_ACTIONCLIENT_HPP
#define RISCV64_THREADX_CPP_ACTION_CLIENT_ACTIONCLIENT_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace riscv64_threadx_cpp_action_client {

class ActionClient {
    ::nros::ActionClientStorage client_;
    ::nros::Timer poll_timer_;
    int32_t order_ = 10;

    void on_goal_response(bool accepted, const uint8_t goal_id[16]);
    void on_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len);
    void on_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len);

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace riscv64_threadx_cpp_action_client

#endif
