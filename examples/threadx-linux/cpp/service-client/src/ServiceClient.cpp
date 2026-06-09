/// @file ServiceClient.cpp
/// @brief C++ ServiceClient component — Phase 212.L Component pkg.

#include <cstdint>

#include <nros/nros.hpp>
#include <nros/node_pkg.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_service_client {

class ServiceClient {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "add_two_ints_client";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredEntity cli;
        return node.create_service_client(cli, "/add_two_ints",
                                          "example_interfaces/srv/AddTwoInts");
    }
};

} // namespace threadx_linux_cpp_service_client

NROS_NODE_REGISTER(threadx_linux_cpp_service_client::ServiceClient,
                   "threadx_linux_cpp_service_client::ServiceClient");
