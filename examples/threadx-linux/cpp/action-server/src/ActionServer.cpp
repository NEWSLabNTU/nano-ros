/// @file ActionServer.cpp
/// @brief C++ ActionServer component — Phase 212.L Component pkg.
///
/// Declares an `example_interfaces/Fibonacci` action server on
/// `/fibonacci`. Body callbacks (goal/cancel/accepted) land with W.5.6
/// plumbing; this file is the declarative SSoT.

#include <cstdint>

#include <nros/node_pkg.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_action_server {

class ActionServer {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "fibonacci_action_server";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::NodeEntityDescriptor act{};
        act.stable_id = "act_fib";
        act.node_id = "node";
        act.kind = nros::NodeEntityKind::ActionServer;
        act.source_name = "/fibonacci";
        act.type_name = "example_interfaces/action/Fibonacci";
        act.type_hash = "";
        act.callback_id = "on_goal";
        return node.create_entity(act);
    }
};

} // namespace threadx_linux_cpp_action_server

NROS_NODE_REGISTER(threadx_linux_cpp_action_server::ActionServer,
                   "threadx_linux_cpp_action_server::ActionServer");
