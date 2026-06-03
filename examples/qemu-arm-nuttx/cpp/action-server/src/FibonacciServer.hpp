// Phase 212.L Component pkg — NuttX C++ Fibonacci action server.
#ifndef NUTTX_CPP_ACTION_SERVER_FIBONACCISERVER_HPP
#define NUTTX_CPP_ACTION_SERVER_FIBONACCISERVER_HPP

#include <nros/node_pkg.hpp>

namespace nuttx_cpp_action_server {

class FibonacciServer {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace nuttx_cpp_action_server

#endif
