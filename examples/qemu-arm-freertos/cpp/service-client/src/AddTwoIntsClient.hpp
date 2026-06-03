// Phase 212.L Component pkg — FreeRTOS QEMU C++ AddTwoInts client.
#ifndef FREERTOS_CPP_SERVICE_CLIENT_ADDTWOINTS_CLIENT_HPP
#define FREERTOS_CPP_SERVICE_CLIENT_ADDTWOINTS_CLIENT_HPP

#include <nros/node_pkg.hpp>

namespace freertos_cpp_service_client {

class AddTwoIntsClient {
  public:
    static ::nros::Result register_node(::nros::NodeContext& context);
};

} // namespace freertos_cpp_service_client

#endif // FREERTOS_CPP_SERVICE_CLIENT_ADDTWOINTS_CLIENT_HPP
