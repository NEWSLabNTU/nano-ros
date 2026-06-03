// Phase 212.L Component pkg — FreeRTOS QEMU C++ listener.
#ifndef FREERTOS_CPP_LISTENER_LISTENER_HPP
#define FREERTOS_CPP_LISTENER_LISTENER_HPP

#include <nros/node_pkg.hpp>

namespace freertos_cpp_listener {

class Listener {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace freertos_cpp_listener

#endif // FREERTOS_CPP_LISTENER_LISTENER_HPP
