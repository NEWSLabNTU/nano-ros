// Phase 212.L Component pkg — NuttX C++ AddTwoInts service server.
#ifndef NUTTX_CPP_SERVICE_SERVER_ADDTWOINTSSERVER_HPP
#define NUTTX_CPP_SERVICE_SERVER_ADDTWOINTSSERVER_HPP

#include <nros/node_pkg.hpp>

namespace nuttx_cpp_service_server {

class AddTwoIntsServer {
  public:
    static ::nros::Result register_node(::nros::NodeContext& context);
};

} // namespace nuttx_cpp_service_server

#endif
