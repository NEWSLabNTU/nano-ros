/// @file ActionServer.cpp
/// @brief C++ ActionServer component — Phase 212.L Component pkg.
///
/// Declares an `example_interfaces/Fibonacci` action server on
/// `/fibonacci`. Body callbacks (goal/cancel/accepted) land with W.5.6
/// plumbing; this file is the declarative SSoT.

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_action_server {

class ActionServer {
  public:
    static nros::Result register_component(nros::ComponentContext& context) {
        nros::ComponentNode node;
        nros::NodeOptions options;
        options.name = "fibonacci_action_server";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::ComponentEntityDescriptor act{};
        act.id = "act_fib";
        act.kind = nros::EntityKind::ActionServer;
        act.source_name = "/fibonacci";
        act.type_name = example_interfaces::action::Fibonacci::ACTION_NAME;
        act.type_hash = example_interfaces::action::Fibonacci::ACTION_HASH;
        act.callback_id = "on_goal";
        return node.create_entity(act);
    }
};

} // namespace threadx_linux_cpp_action_server

NROS_COMPONENT_REGISTER(threadx_linux_cpp_action_server::ActionServer,
                        "threadx_linux_cpp_action_server::ActionServer");
