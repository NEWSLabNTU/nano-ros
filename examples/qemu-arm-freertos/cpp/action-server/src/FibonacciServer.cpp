/// @file FibonacciServer.cpp
/// @brief FreeRTOS QEMU MPS2-AN385 C++ Fibonacci action server —
///        Phase 212.L Component pkg.

#include "FibonacciServer.hpp"

namespace freertos_cpp_action_server {

::nros::Result FibonacciServer::register_component(::nros::ComponentContext& ctx) {
    ::nros::ComponentNode node;
    auto opts = ::nros::NodeOptions::make("fibonacci_action_server");
    auto r = ctx.create_node(node, "node", opts);
    if (!r.ok()) return r;

    ::nros::ComponentEntityDescriptor action{
        "act_fib",
        "node",
        ::nros::ComponentEntityKind::ActionServer,
        "/fibonacci",
        "example_interfaces/action/Fibonacci",
        "",
        "on_goal",
    };
    return node.create_entity(action);
}

} // namespace freertos_cpp_action_server

NROS_COMPONENT_REGISTER(freertos_cpp_action_server::FibonacciServer,
                        "freertos_cpp_action_server::FibonacciServer");
