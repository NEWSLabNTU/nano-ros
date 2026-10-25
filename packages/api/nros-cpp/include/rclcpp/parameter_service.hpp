// nros-cpp: <rclcpp/parameter_service.hpp>
//
// phase-488 — upstream's `rclcpp::ParameterService` lives here. nros-cpp
// serves the six parameter services from the node itself
// (`Node::start_parameter_services`, started by the constructor on an
// ament-built host target), so there is no class to declare. Ported code
// includes the header without naming the class (Nav2's
// `nav2_util/twist_publisher.hpp`), and a missing header stopped the compile
// at the include line. A source that names `rclcpp::ParameterService` still
// fails, on the name.

#ifndef NROS_RCLCPP_PARAMETER_SERVICE_HPP
#define NROS_RCLCPP_PARAMETER_SERVICE_HPP

#include "rclcpp/rclcpp.hpp"

#endif // NROS_RCLCPP_PARAMETER_SERVICE_HPP
