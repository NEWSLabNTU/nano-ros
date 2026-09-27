# Find-stub for rclcpp_components — Phase 209.B (NrosRclcppCompat).
#
# Defines `rclcpp_components::component` as an IMPORTED INTERFACE forwarding to
# NanoRos::NanoRosCpp so the typical `target_link_libraries(<lib>
# rclcpp_components::component)` on a component-style ported source resolves.
# The `rclcpp_components_register_node()` cmake macro itself is defined by
# `NrosRclcppCompat.cmake` (single-binary embedded; emits a thin `int main()`
# that constructs the registered class + `rclcpp::spin`s it).
# issue 1467 — one home for the umbrella choice; see the note in Findrclcpp.cmake.
include("${CMAKE_CURRENT_LIST_DIR}/../../NanoRosRuntimeUmbrella.cmake")
if(NOT TARGET rclcpp_components::component)
    add_library(rclcpp_components::component INTERFACE IMPORTED)
    nros_link_runtime_umbrella(rclcpp_components::component INTERFACE
        CANDIDATES NanoRos::NanoRosCpp)
endif()
# Some sources use a `::rclcpp_components` alias instead.
if(NOT TARGET rclcpp_components::rclcpp_components)
    add_library(rclcpp_components::rclcpp_components INTERFACE IMPORTED)
    target_link_libraries(rclcpp_components::rclcpp_components INTERFACE rclcpp_components::component)
endif()
set(rclcpp_components_FOUND TRUE)
