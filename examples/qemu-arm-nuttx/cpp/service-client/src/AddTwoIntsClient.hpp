// Phase 212.L Component pkg — NuttX C++ AddTwoInts service client.
#ifndef NUTTX_CPP_SERVICE_CLIENT_ADDTWOINTSCLIENT_HPP
#define NUTTX_CPP_SERVICE_CLIENT_ADDTWOINTSCLIENT_HPP

#include <nros/component.hpp>

namespace nuttx_cpp_service_client {

class AddTwoIntsClient {
  public:
    static ::nros::Result register_component(::nros::ComponentContext& context);
};

} // namespace nuttx_cpp_service_client

#endif
