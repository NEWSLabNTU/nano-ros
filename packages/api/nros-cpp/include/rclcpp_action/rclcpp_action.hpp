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
 * Issue 2015 — it also brings upstream's free `create_client` /
 * `create_server` (their own headers, at upstream's paths). What a ported
 * action node still rewrites is listed in the porting guide.
 */

#ifndef NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP
#define NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP

#include "rclcpp/rclcpp.hpp"
#include "rclcpp_action/client.hpp"
#include "rclcpp_action/server.hpp"
// issue 2015 — upstream's free factories, at upstream's paths.
#include "rclcpp_action/create_client.hpp"
#include "rclcpp_action/create_server.hpp"

#endif // NROS_RCLCPP_ACTION_RCLCPP_ACTION_HPP
