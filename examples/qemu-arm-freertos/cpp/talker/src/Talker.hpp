// Phase 212.L Component pkg — FreeRTOS QEMU C++ talker.
#ifndef FREERTOS_CPP_TALKER_TALKER_HPP
#define FREERTOS_CPP_TALKER_TALKER_HPP

#include <nros/component.hpp>

namespace freertos_cpp_talker {

class Talker {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace freertos_cpp_talker

#endif // FREERTOS_CPP_TALKER_TALKER_HPP
