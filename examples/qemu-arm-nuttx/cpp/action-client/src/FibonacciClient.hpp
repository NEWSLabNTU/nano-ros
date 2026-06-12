// NuttX C++ Fibonacci action client — typed poll component (RFC-0043, 240.5).
//
// `configure` creates an action client + a timer that drives a poll state
// machine: send goal → poll the acceptance → fetch the result → print it.
// (Poll model — clients move to callbacks when RFC-0041's C/C++ wave lands.)
#ifndef NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP
#define NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP

#include <cstddef>
#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

namespace nuttx_cpp_action_client {

class FibonacciClient {
    ::nros::ActionClientStorage client_;
    ::nros::Timer timer_;
    void* executor_ = nullptr;
    int phase_ = 0; // 0 send, 1 await-accept, 2 get-result, 3 done
    int32_t order_ = 5;
    uint8_t goal_id_[16] = {};

    void on_tick(); // poll state machine, bound by identity

  public:
    ::nros::Result configure(::nros::Node& node);
};

} // namespace nuttx_cpp_action_client

#endif
