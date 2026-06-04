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
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::NodeEntityDescriptor srv{};
        srv.stable_id = "srv_add";
        srv.node_id = "node";
        srv.kind = nros::NodeEntityKind::ServiceServer;
        srv.source_name = "/add_two_ints";
        srv.type_name = "example_interfaces/srv/AddTwoInts";
        srv.type_hash = "";
        srv.callback_id = "on_add";
        return node.create_entity(srv);
    }
};

} // namespace threadx_linux_cpp_service_server

NROS_NODE_REGISTER(threadx_linux_cpp_service_server::ServiceServer,
                   "threadx_linux_cpp_service_server::ServiceServer");
