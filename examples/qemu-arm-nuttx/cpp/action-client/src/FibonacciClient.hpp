// Phase 212.L Component pkg — NuttX C++ Fibonacci action client.
#ifndef NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP
#define NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP

#include <nros/component.hpp>

namespace nuttx_cpp_action_client {

class FibonacciClient {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace nuttx_cpp_action_client

#endif
