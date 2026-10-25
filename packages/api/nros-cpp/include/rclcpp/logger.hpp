// nros-cpp: <rclcpp/logger.hpp>
//
// phase-488 — upstream declares `rclcpp::Logger` and `rclcpp::get_logger` in
// this header, and ported code includes it by name (Nav2's `nav2_util`
// `robot_utils.cpp`). nros-cpp declares both in `rclcpp/logging.hpp`, so this
// header is that one under upstream's name.

#ifndef NROS_RCLCPP_LOGGER_HPP
#define NROS_RCLCPP_LOGGER_HPP

#include "rclcpp/logging.hpp"

#endif // NROS_RCLCPP_LOGGER_HPP
