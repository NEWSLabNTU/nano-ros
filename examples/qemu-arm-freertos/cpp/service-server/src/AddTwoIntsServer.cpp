/// @file AddTwoIntsServer.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ AddTwoInts service server —
///        Phase 212.L Component pkg.

#include "AddTwoIntsServer.hpp"

namespace freertos_cpp_service_server {

::nros::Result AddTwoIntsServer::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("add_two_ints_server");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor srv{
        "srv_add",
        "node",
        ::nros::NodeEntityKind::ServiceServer,
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "",
        "handle_add",
    };
    return node.create_entity(srv);
}

} // namespace freertos_cpp_service_server

NROS_NODE_REGISTER(freertos_cpp_service_server::AddTwoIntsServer,
                        "freertos_cpp_service_server::AddTwoIntsServer");
