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
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredEntity client;
    return node.create_service_client(client, "/add_two_ints", "example_interfaces/srv/AddTwoInts");
}

} // namespace freertos_cpp_service_client

NROS_NODE_REGISTER(freertos_cpp_service_client::AddTwoIntsClient,
                   "freertos_cpp_service_client::AddTwoIntsClient");
