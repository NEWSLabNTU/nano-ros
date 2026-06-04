/// @file ActionClient.cpp
/// @brief C++ ActionClient component — Phase 212.L Component pkg.

#include <cstdint>

#include <nros/node_pkg.hpp>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

namespace threadx_linux_cpp_action_client {

class ActionClient {
  public:
    static nros::Result register_node(nros::NodeContext& context) {
        nros::DeclaredNode node;
        nros::NodeOptions options;
        options.name = "fibonacci_action_client";
        options.namespace_ = "/";
        nros::Result rc = context.create_node(node, "node", options);
        if (!rc.ok()) return rc;

        nros::NodeEntityDescriptor cli{};
        cli.stable_id = "cli_fib";
        cli.node_id = "node";
        cli.kind = nros::NodeEntityKind::ActionClient;
        cli.source_name = "/fibonacci";
        cli.type_name = "example_interfaces/action/Fibonacci";
        cli.type_hash = "";
        return node.create_entity(cli);
    }
};

} // namespace threadx_linux_cpp_action_client

NROS_NODE_REGISTER(threadx_linux_cpp_action_client::ActionClient,
                   "threadx_linux_cpp_action_client::ActionClient");
