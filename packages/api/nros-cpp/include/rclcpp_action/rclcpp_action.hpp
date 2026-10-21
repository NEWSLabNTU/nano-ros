// nros-cpp: `<rclcpp_action/rclcpp_action.hpp>` umbrella
// Freestanding C++ — no exceptions, no STL required

/**
 * @file rclcpp_action.hpp
 * @ingroup grp_action
 * @brief The `rclcpp_action` umbrella upstream sources include.
 *
 * phase-488 — every stock rclcpp action file opens with
 * `#include "rclcpp_action/rclcpp_action.hpp"`, and that path did not exist
 * here, so the include line was an edit in every ported action node. It
 * declares nothing of its own: `rclcpp_action::Server<A>` / `Client<A>` live in
 * `server.hpp` / `client.hpp`, which `<rclcpp/rclcpp.hpp>` already includes.
 * What a ported action node still rewrites (goal handles, futures) is listed in
 * the porting guide.
 */

#ifndef NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP
#define NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/client.hpp"
#include "rclcpp_action/server.hpp"

#endif // NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP
