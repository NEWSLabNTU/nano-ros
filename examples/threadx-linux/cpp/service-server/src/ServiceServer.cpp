/// @file ServiceServer.cpp
/// @brief C++ ServiceServer component — Phase 212.L Component pkg.
///
/// Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#include <cstdint>

#include <nros/node_pkg.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_service_server {

class ServiceServer {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "add_two_ints_server";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredCallback on_add;
        rc = node.declare_callback(on_add, "on_add");
        if (!rc.ok()) return rc;

        nros::DeclaredEntity srv;
        return node.create_service_server(srv, "/add_two_ints", "example_interfaces/srv/AddTwoInts",
                                          on_add);
    }
};

} // namespace threadx_linux_cpp_service_server

NROS_NODE_REGISTER(threadx_linux_cpp_service_server::ServiceServer,
                   "threadx_linux_cpp_service_server::ServiceServer");
