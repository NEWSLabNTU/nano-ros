// SPDX-License-Identifier: Apache-2.0
//
// rclcpp_components_compat.hpp — Phase 209.C
//
// nano-ros is single-binary embedded; there is no runtime ComponentManager,
// so the macro `RCLCPP_COMPONENTS_REGISTER_NODE(<class>)` (which upstream
// emits a class-loader plugin registration TU) becomes a no-op. The cmake-
// side `rclcpp_components_register_node()` in NrosRclcppCompat.cmake (Phase
// 209.B) synthesises a thin `int main()` per `EXECUTABLE` arg that constructs
// the registered class + `rclcpp::spin`s it — that's how composability is
// modeled here. The C++ source's macro invocation just compiles away.

#ifndef NROS_RCLCPP_COMPONENTS_COMPAT_HPP
#define NROS_RCLCPP_COMPONENTS_COMPAT_HPP

#ifndef RCLCPP_COMPONENTS_REGISTER_NODE
#define RCLCPP_COMPONENTS_REGISTER_NODE(NodeClass) /* no-op (nano-ros single-binary) */
#endif

#endif // NROS_RCLCPP_COMPONENTS_COMPAT_HPP
