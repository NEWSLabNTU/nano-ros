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
        nros::Result rc = context.create_node(node, options);
        if (!rc.ok()) return rc;

        nros::DeclaredEntity cli;
        return node.create_action_client(cli, "/fibonacci", "example_interfaces/action/Fibonacci");
    }
};

} // namespace threadx_linux_cpp_action_client

NROS_NODE_REGISTER(threadx_linux_cpp_action_client::ActionClient,
                   "threadx_linux_cpp_action_client::ActionClient");
