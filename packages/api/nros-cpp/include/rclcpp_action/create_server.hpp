// nros-cpp: `rclcpp_action::create_server` — upstream's free factory
// Freestanding C++ — no exceptions, no STL required

/**
 * @file create_server.hpp
 * @ingroup grp_action
 * @brief `rclcpp_action::create_server<A>(...)` — issue 2015.
 *
 * Upstream's free function, at upstream's path, taking upstream's three
 * callbacks — `handle_goal(const GoalUUID&, std::shared_ptr<const Goal>)`,
 * `handle_cancel(std::shared_ptr<ServerGoalHandle<A>>)` and
 * `handle_accepted(std::shared_ptr<ServerGoalHandle<A>>)`, typically
 * `std::bind`-ed members — and returning `Server<A>::SharedPtr`
 * (`rclcpp::Owned<Server<A>>`, the server by value). HOSTED only: the
 * callbacks are `std::function`s and each goal handle is a heap object; a
 * freestanding build refuses the call (`NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE`)
 * and keeps `Node::create_action_server` + `set_*_callback_with_ctx`.
 *
 * Two upstream shapes: the four node interfaces (what the stock examples
 * pass) and a node (`create_server<A>(node, name, ...)`). A failed create
 * ABORTS naming the action (`detail::require_created`; RFC-0018). Upstream's
 * trailing `rcl_action_server_options_t` and callback-group arguments are not
 * taken.
 */

#ifndef NROS_RCLCPP_ACTION_CREATE_SERVER_HPP
#define NROS_RCLCPP_ACTION_CREATE_SERVER_HPP

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/server.hpp"

namespace rclcpp_action {

/// `create_server<A>(base, clock, logging, waitables, name, goal, cancel,
/// accepted)` — upstream's node-interface shape. All four handles name the
/// same node.
template <typename A, typename GoalCb, typename CancelCb, typename AcceptedCb>
typename Server<A>::SharedPtr
create_server(::rclcpp::node_interfaces::NodeBaseInterface::SharedPtr node_base,
              ::rclcpp::node_interfaces::NodeClockInterface::SharedPtr /*node_clock*/,
              ::rclcpp::node_interfaces::NodeLoggingInterface::SharedPtr /*node_logging*/,
              ::rclcpp::node_interfaces::NodeWaitablesInterface::SharedPtr /*node_waitables*/,
              const char* name, GoalCb handle_goal, CancelCb handle_cancel,
              AcceptedCb handle_accepted) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    Server<A> server;
    ::rclcpp::Result r = node_base ? node_base.node()->create_action_server(server, name)
                                   : ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
    if (r.ok()) {
        r = detail::ServerUpstream<A>::bind(server, static_cast<GoalCb&&>(handle_goal),
                                            static_cast<CancelCb&&>(handle_cancel),
                                            static_cast<AcceptedCb&&>(handle_accepted));
    }
    ::rclcpp::detail::require_created(r, "rclcpp_action::create_server", name);
    return typename Server<A>::SharedPtr(static_cast<Server<A>&&>(server));
#else
    static_assert(::rclcpp::detail::refuse<GoalCb>::value, NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE);
    (void)node_base;
    (void)name;
    (void)handle_goal;
    (void)handle_cancel;
    (void)handle_accepted;
    return typename Server<A>::SharedPtr();
#endif
}

/// `create_server<A>(node, name, goal, cancel, accepted)` — upstream's node
/// shape (`node` a `Node*` or `Node::SharedPtr`).
template <typename A, typename NodeT, typename GoalCb, typename CancelCb, typename AcceptedCb>
typename Server<A>::SharedPtr create_server(NodeT node, const char* name, GoalCb handle_goal,
                                            CancelCb handle_cancel, AcceptedCb handle_accepted) {
    return create_server<A>(
        node->get_node_base_interface(), node->get_node_clock_interface(),
        node->get_node_logging_interface(), node->get_node_waitables_interface(), name,
        static_cast<GoalCb&&>(handle_goal), static_cast<CancelCb&&>(handle_cancel),
        static_cast<AcceptedCb&&>(handle_accepted));
}

#ifdef NROS_CPP_HAS_STD_STRING // hosted-family: string-interop
/// `std::string` name overloads of the two above.
template <typename A, typename GoalCb, typename CancelCb, typename AcceptedCb>
typename Server<A>::SharedPtr
create_server(::rclcpp::node_interfaces::NodeBaseInterface::SharedPtr node_base,
              ::rclcpp::node_interfaces::NodeClockInterface::SharedPtr node_clock,
              ::rclcpp::node_interfaces::NodeLoggingInterface::SharedPtr node_logging,
              ::rclcpp::node_interfaces::NodeWaitablesInterface::SharedPtr node_waitables,
              const ::std::string& name, GoalCb handle_goal, CancelCb handle_cancel,
              AcceptedCb handle_accepted) {
    return create_server<A>(node_base, node_clock, node_logging, node_waitables, name.c_str(),
                            static_cast<GoalCb&&>(handle_goal),
                            static_cast<CancelCb&&>(handle_cancel),
                            static_cast<AcceptedCb&&>(handle_accepted));
}

template <typename A, typename NodeT, typename GoalCb, typename CancelCb, typename AcceptedCb>
typename Server<A>::SharedPtr create_server(NodeT node, const ::std::string& name,
                                            GoalCb handle_goal, CancelCb handle_cancel,
                                            AcceptedCb handle_accepted) {
    return create_server<A>(node, name.c_str(), static_cast<GoalCb&&>(handle_goal),
                            static_cast<CancelCb&&>(handle_cancel),
                            static_cast<AcceptedCb&&>(handle_accepted));
}
#endif

} // namespace rclcpp_action

#endif // NROS_RCLCPP_ACTION_CREATE_SERVER_HPP
