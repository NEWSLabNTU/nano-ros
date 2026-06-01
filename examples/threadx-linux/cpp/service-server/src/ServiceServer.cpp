/// @file ServiceServer.cpp
/// @brief C++ ServiceServer component — Phase 212.L Component pkg.
///
/// Handles `example_interfaces/AddTwoInts` requests on `/add_two_ints`.

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_service_server {

class ServiceServer {
  public:
    static nros::Result register_component(nros::ComponentContext& context) {
        nros::ComponentNode node;
        nros::NodeOptions options;
        options.name = "add_two_ints_server";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::ComponentEntityDescriptor srv{};
        srv.id = "srv_add";
        srv.kind = nros::EntityKind::ServiceServer;
        srv.source_name = "/add_two_ints";
        srv.type_name = example_interfaces::srv::AddTwoInts::SERVICE_NAME;
        srv.type_hash = example_interfaces::srv::AddTwoInts::SERVICE_HASH;
        srv.callback_id = "on_add";
        return node.create_entity(srv);
    }
};

} // namespace threadx_linux_cpp_service_server

NROS_COMPONENT_REGISTER(threadx_linux_cpp_service_server::ServiceServer,
                        "threadx_linux_cpp_service_server::ServiceServer");
