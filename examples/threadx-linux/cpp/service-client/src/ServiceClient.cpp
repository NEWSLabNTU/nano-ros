/// @file ServiceClient.cpp
/// @brief C++ ServiceClient component — Phase 212.L Component pkg.

#include <cstdint>

#include <nros/node_pkg.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_service_client {

class ServiceClient {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "add_two_ints_client";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::NodeEntityDescriptor cli{};
        cli.stable_id = "cli_add";
        cli.node_id = "node";
        cli.kind = nros::NodeEntityKind::ServiceClient;
        cli.source_name = "/add_two_ints";
        cli.type_name = "example_interfaces/srv/AddTwoInts";
        cli.type_hash = "";
        return node.create_entity(cli);
    }
};

} // namespace threadx_linux_cpp_service_client

NROS_NODE_REGISTER(threadx_linux_cpp_service_client::ServiceClient,
                   "threadx_linux_cpp_service_client::ServiceClient");
