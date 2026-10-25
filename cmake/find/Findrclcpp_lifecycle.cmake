# Find module for rclcpp_lifecycle — phase-488; part of nano-ros's CMake package.
#
# nros-cpp ships `<rclcpp_lifecycle/...>` as headers of its own, so the package is
# rclcpp's: `rclcpp_lifecycle::rclcpp_lifecycle` is an IMPORTED INTERFACE forwarding to
# `rclcpp::rclcpp`, which carries the runtime and the std-surface define.
# Without this module `find_package(rclcpp_lifecycle)` (Nav2's `nav2_util`) fell to
# CONFIG mode and loaded the installed upstream library's CMake.
if(NOT TARGET rclcpp::rclcpp)
    include("${CMAKE_CURRENT_LIST_DIR}/Findrclcpp.cmake")
endif()
if(NOT TARGET rclcpp_lifecycle::rclcpp_lifecycle)
    add_library(rclcpp_lifecycle::rclcpp_lifecycle INTERFACE IMPORTED)
    target_link_libraries(rclcpp_lifecycle::rclcpp_lifecycle INTERFACE rclcpp::rclcpp)
endif()
set(rclcpp_lifecycle_FOUND TRUE)
