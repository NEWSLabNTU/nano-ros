/// @file ActionServer.cpp
/// @brief C++ ActionServer component — Phase 212.L Component pkg.
///
/// Declares an `example_interfaces/Fibonacci` action server on
/// `/fibonacci`. Body callbacks (goal/cancel/accepted) land with W.5.6
/// plumbing; this file is the declarative SSoT.

#include <cstdint>

#include <nros/nros.hpp>
#include <nros/node_pkg.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_action_server {

class ActionServer {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "fibonacci_action_server";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredCallback on_goal;
        rc = node.declare_callback(on_goal, "on_goal");
        if (!rc.ok()) return rc;

        nros::DeclaredEntity act;
        return node.create_action_server(act, "/fibonacci", "example_interfaces/action/Fibonacci",
                                         on_goal);
    }
};

} // namespace threadx_linux_cpp_action_server

NROS_NODE_REGISTER(threadx_linux_cpp_action_server::ActionServer,
                   "threadx_linux_cpp_action_server::ActionServer");
