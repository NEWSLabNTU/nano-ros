// SPDX-License-Identifier: Apache-2.0
// <rclcpp_components/register_node_macro.hpp>
//
// nano-ros is single-binary: there is no runtime ComponentManager and no
// class-loader plugin index, so `RCLCPP_COMPONENTS_REGISTER_NODE(<class>)`
// compiles away. Composition is modelled at build time instead:
// `rclcpp_components_register_node(... EXECUTABLE <bin>)` in
// `cmake/NanoRosAmentSurface.cmake` synthesises a `main()` that constructs the
// registered class and spins it.
#ifndef NROS_RCLCPP_COMPONENTS_REGISTER_NODE_MACRO_HPP
#define NROS_RCLCPP_COMPONENTS_REGISTER_NODE_MACRO_HPP
#ifndef RCLCPP_COMPONENTS_REGISTER_NODE
#define RCLCPP_COMPONENTS_REGISTER_NODE(NodeClass) /* no-op (nano-ros single-binary) */
#endif
#endif
