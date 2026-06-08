/// @file FibonacciClient.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ Fibonacci action client —
///        Phase 212.L Component pkg.
///
/// Phase 212.M.5.b — declarative-metadata-only.
/// Service-client runtime body deferred to M-F.4 (TickCtx call() seam) —
/// the same dependency applies to action-client send_goal /
/// feedback-stream wiring.

#include "FibonacciClient.hpp"

namespace freertos_cpp_action_client {

::nros::Result FibonacciClient::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("fibonacci_action_client");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredEntity client;
    return node.create_action_client(client, "/fibonacci", "example_interfaces/action/Fibonacci");
}

} // namespace freertos_cpp_action_client

NROS_NODE_REGISTER(freertos_cpp_action_client::FibonacciClient,
                   "freertos_cpp_action_client::FibonacciClient");
