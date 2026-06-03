// Phase 212.L Component pkg — NuttX C++ Fibonacci action client.
#ifndef NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP
#define NUTTX_CPP_ACTION_CLIENT_FIBONACCICLIENT_HPP

#include <nros/node_pkg.hpp>

namespace nuttx_cpp_action_client {

class FibonacciClient {
  public:
    static ::nros::Result register_node(::nros::NodeContext& context);
};

} // namespace nuttx_cpp_action_client

#endif
