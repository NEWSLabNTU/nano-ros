/// @file AddTwoIntsClient.cpp
/// @brief NuttX C++ AddTwoInts service client — Phase 212.L Component pkg.
///
/// Declarative metadata; imperative call sequencing follows once the
/// runtime grows a TickCtx service-client seam.

#include "AddTwoIntsClient.hpp"

namespace nuttx_cpp_service_client {

::nros::Result AddTwoIntsClient::register_component(::nros::ComponentContext& ctx) {
    ::nros::ComponentNode node;
    auto opts = ::nros::NodeOptions::make("add_two_ints_client");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::ComponentEntityDescriptor client{
        "client_add",
        "node",
        ::nros::ComponentEntityKind::ServiceClient,
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "",
        nullptr,
    };
    return node.create_entity(client);
}

} // namespace nuttx_cpp_service_client

NROS_NODE_REGISTER(nuttx_cpp_service_client::AddTwoIntsClient,
                        "nuttx_cpp_service_client::AddTwoIntsClient");
