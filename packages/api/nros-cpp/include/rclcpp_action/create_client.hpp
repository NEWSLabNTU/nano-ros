// nros-cpp: `rclcpp_action::create_client` — upstream's free factory
// Freestanding C++ — no exceptions, no STL required

/**
 * @file create_client.hpp
 * @ingroup grp_action
 * @brief `rclcpp_action::create_client<A>(...)` — issue 2015.
 *
 * Upstream's free function, at upstream's path. It builds the client the
 * method `Node::create_action_client(out, name)` builds, and returns it as
 * `Client<A>::SharedPtr` — `rclcpp::Owned<Client<A>>`, the client by value, so
 * this allocates nothing and is available on every build. Two upstream
 * shapes: the four node interfaces (what the stock examples pass) and a node
 * (`create_client<A>(node, name)`, `node` a `Node*` or `Node::SharedPtr`).
 * A failed create ABORTS naming the action (`detail::require_created`), as
 * every `SharedPtr`-returning factory here does (RFC-0018: no exceptions).
 * Upstream's trailing callback-group and `rcl_action_client_options_t`
 * arguments are not taken.
 */

#ifndef NROS_RCLCPP_ACTION_CREATE_CLIENT_HPP
#define NROS_RCLCPP_ACTION_CREATE_CLIENT_HPP

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/client.hpp"

namespace rclcpp_action {

/// `create_client<A>(base, graph, logging, waitables, name)` — upstream's
/// node-interface shape. All four handles name the same node.
template <typename A>
typename Client<A>::SharedPtr
create_client(::rclcpp::node_interfaces::NodeBaseInterface::SharedPtr node_base,
              ::rclcpp::node_interfaces::NodeGraphInterface::SharedPtr /*node_graph*/,
              ::rclcpp::node_interfaces::NodeLoggingInterface::SharedPtr /*node_logging*/,
              ::rclcpp::node_interfaces::NodeWaitablesInterface::SharedPtr /*node_waitables*/,
              const char* name) {
    Client<A> client;
    ::rclcpp::Result r = node_base ? node_base.node()->create_action_client(client, name)
                                   : ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
    ::rclcpp::detail::require_created(r, "rclcpp_action::create_client", name);
    return typename Client<A>::SharedPtr(static_cast<Client<A>&&>(client));
}

/// `create_client<A>(node, name)` — upstream's node shape. `NodeT` is anything
/// with `->get_node_base_interface()`: a `Node*` (`this`) or a
/// `Node::SharedPtr`.
template <typename A, typename NodeT>
typename Client<A>::SharedPtr create_client(NodeT node, const char* name) {
    return create_client<A>(node->get_node_base_interface(), node->get_node_graph_interface(),
                            node->get_node_logging_interface(),
                            node->get_node_waitables_interface(), name);
}

#ifdef NROS_CPP_HAS_STD_STRING // hosted-family: string-interop
/// `std::string` name overloads of the two above.
template <typename A>
typename Client<A>::SharedPtr
create_client(::rclcpp::node_interfaces::NodeBaseInterface::SharedPtr node_base,
              ::rclcpp::node_interfaces::NodeGraphInterface::SharedPtr node_graph,
              ::rclcpp::node_interfaces::NodeLoggingInterface::SharedPtr node_logging,
              ::rclcpp::node_interfaces::NodeWaitablesInterface::SharedPtr node_waitables,
              const ::std::string& name) {
    return create_client<A>(node_base, node_graph, node_logging, node_waitables, name.c_str());
}

template <typename A, typename NodeT>
typename Client<A>::SharedPtr create_client(NodeT node, const ::std::string& name) {
    return create_client<A>(node, name.c_str());
}
#endif

} // namespace rclcpp_action

#endif // NROS_RCLCPP_ACTION_CREATE_CLIENT_HPP
