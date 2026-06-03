/// @file FibonacciServer.cpp
/// @brief NuttX C++ Fibonacci action server — Phase 212.L Component pkg.

#include "FibonacciServer.hpp"

namespace nuttx_cpp_action_server {

::nros::Result FibonacciServer::register_node(::nros::NodeContext& ctx) {
    ::nros::DeclaredNode node;
    auto opts = ::nros::NodeOptions::make("fibonacci_action_server");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::NodeEntityDescriptor action{
        "act_fib",
        "node",
        ::nros::NodeEntityKind::ActionServer,
        "/fibonacci",
        "example_interfaces/action/Fibonacci",
        "",
        "on_goal",
    };
    return node.create_entity(action);
}

} // namespace nuttx_cpp_action_server

NROS_NODE_REGISTER(nuttx_cpp_action_server::FibonacciServer,
                        "nuttx_cpp_action_server::FibonacciServer");
