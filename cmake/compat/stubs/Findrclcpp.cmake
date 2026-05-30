# Find-stub for rclcpp — Phase 209.B (NrosRclcppCompat) + Phase 210.A.4 update.
#
# Defines the `rclcpp::rclcpp` IMPORTED INTERFACE target that ROS 2 cmake
# typically links against (`target_link_libraries(my_target rclcpp::rclcpp)`),
# transparently forwarding to NanoRos::NanoRosCpp so the link resolves to the
# nano-ros surface.
#
# Phase 210.A.4 — also publishes the rclcpp_compat shim header path +
# force-include flags on the imported target so a stock `add_executable() +
# target_link_libraries(... rclcpp::rclcpp)` consumer (no ament_auto_*
# routing) gets the source-compat layer automatically. The 209.B
# ament_auto_* shims still apply the same hookup via _nros_compat_apply_
# force_includes; this stub is the second entry point that catches
# upstream-style call sites.
if(NOT TARGET rclcpp::rclcpp)
    add_library(rclcpp::rclcpp INTERFACE IMPORTED)
    if(TARGET NanoRos::NanoRosCpp)
        target_link_libraries(rclcpp::rclcpp INTERFACE NanoRos::NanoRosCpp)
    endif()
    # NrosRclcppCompat lives at `../include/` relative to this stub dir.
    get_filename_component(_nros_compat_inc_dir "${CMAKE_CURRENT_LIST_DIR}/../include" ABSOLUTE)
    target_include_directories(rclcpp::rclcpp INTERFACE "${_nros_compat_inc_dir}")
    target_compile_options(rclcpp::rclcpp INTERFACE
        "$<$<COMPILE_LANGUAGE:CXX>:SHELL:-include nros/rclcpp_compat.hpp>"
        "$<$<COMPILE_LANGUAGE:CXX>:SHELL:-include nros/rclcpp_components_compat.hpp>"
    )
endif()
set(rclcpp_FOUND TRUE)
