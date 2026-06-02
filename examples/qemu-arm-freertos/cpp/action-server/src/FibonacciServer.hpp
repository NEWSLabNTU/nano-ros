// Phase 212.L Component pkg — FreeRTOS QEMU C++ Fibonacci action server.
#ifndef FREERTOS_CPP_ACTION_SERVER_FIBONACCI_SERVER_HPP
#define FREERTOS_CPP_ACTION_SERVER_FIBONACCI_SERVER_HPP

#include <nros/component.hpp>

namespace freertos_cpp_action_server {

class FibonacciServer {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace freertos_cpp_action_server

#endif // FREERTOS_CPP_ACTION_SERVER_FIBONACCI_SERVER_HPP
