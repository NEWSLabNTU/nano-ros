/// @file AddTwoIntsServer.cpp
/// @brief NuttX C++ AddTwoInts service server — Phase 212.L Component pkg.

#include "AddTwoIntsServer.hpp"

namespace nuttx_cpp_service_server {

::nros::Result AddTwoIntsServer::register_component(::nros::ComponentContext& ctx) {
    ::nros::ComponentNode node;
    auto opts = ::nros::NodeOptions::make("add_two_ints_server");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::ComponentEntityDescriptor srv{
        "srv_add",
        "node",
        ::nros::ComponentEntityKind::ServiceServer,
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "",
        "handle_add",
    };
    return node.create_entity(srv);
}

} // namespace nuttx_cpp_service_server

NROS_COMPONENT_REGISTER(nuttx_cpp_service_server::AddTwoIntsServer,
                        "nuttx_cpp_service_server::AddTwoIntsServer");
