/// @file FibonacciClient.cpp
/// @brief NuttX C++ Fibonacci action client — Phase 212.L Component pkg.
///
/// Declarative metadata; imperative goal-send wiring follows once the
/// runtime grows the corresponding action-client TickCtx seam.

#include "FibonacciClient.hpp"

namespace nuttx_cpp_action_client {

::nros::Result FibonacciClient::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("fibonacci_action_client");
    auto r = ctx.create_node(node, opts);
    if (!r.ok()) return r;

    ::nros::DeclaredEntity client;
    return node.create_action_client(client, "/fibonacci", "example_interfaces/action/Fibonacci");
}

} // namespace nuttx_cpp_action_client

NROS_NODE_REGISTER(nuttx_cpp_action_client::FibonacciClient,
                   "nuttx_cpp_action_client::FibonacciClient");
