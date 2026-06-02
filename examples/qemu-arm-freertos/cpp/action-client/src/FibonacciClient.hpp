// Phase 212.L Component pkg — FreeRTOS QEMU C++ Fibonacci action client.
#ifndef FREERTOS_CPP_ACTION_CLIENT_FIBONACCI_CLIENT_HPP
#define FREERTOS_CPP_ACTION_CLIENT_FIBONACCI_CLIENT_HPP

#include <nros/component.hpp>

namespace freertos_cpp_action_client {

class FibonacciClient {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace freertos_cpp_action_client

#endif // FREERTOS_CPP_ACTION_CLIENT_FIBONACCI_CLIENT_HPP
