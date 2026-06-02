// Phase 212.L Component pkg — FreeRTOS QEMU C++ AddTwoInts server.
#ifndef FREERTOS_CPP_SERVICE_SERVER_ADDTWOINTS_SERVER_HPP
#define FREERTOS_CPP_SERVICE_SERVER_ADDTWOINTS_SERVER_HPP

#include <nros/component.hpp>

namespace freertos_cpp_service_server {

class AddTwoIntsServer {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace freertos_cpp_service_server

#endif // FREERTOS_CPP_SERVICE_SERVER_ADDTWOINTS_SERVER_HPP
