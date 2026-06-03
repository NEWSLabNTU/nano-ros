/// @file AddTwoIntsClient.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ AddTwoInts service client —
///        Phase 212.L Component pkg.
///
/// Phase 212.M.5.b — declarative-metadata-only.
/// Service-client runtime body deferred to M-F.4 (TickCtx call() seam).

#include "AddTwoIntsClient.hpp"

namespace freertos_cpp_service_client {

::nros::Result AddTwoIntsClient::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("add_two_ints_client");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor client{
        "client_add",
        "node",
        ::nros::NodeEntityKind::ServiceClient,
        "/add_two_ints",
        "example_interfaces/srv/AddTwoInts",
        "",
        nullptr,
    };
    return node.create_entity(client);
}

} // namespace freertos_cpp_service_client

NROS_NODE_REGISTER(freertos_cpp_service_client::AddTwoIntsClient,
                        "freertos_cpp_service_client::AddTwoIntsClient");
