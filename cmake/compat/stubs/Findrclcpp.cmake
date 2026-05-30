# Find-stub for rclcpp — Phase 209.B (NrosRclcppCompat).
#
# Defines the `rclcpp::rclcpp` IMPORTED INTERFACE target that ROS 2 cmake
# typically links against (`target_link_libraries(my_target rclcpp::rclcpp)`),
# transparently forwarding to NanoRos::NanoRosCpp so the link resolves to the
# nano-ros surface. The C++ source-compat is supplied by
# `nros/rclcpp_compat.hpp` which the NrosRclcppCompat module force-includes
# on every compat-built target.
if(NOT TARGET rclcpp::rclcpp)
    add_library(rclcpp::rclcpp INTERFACE IMPORTED)
    if(TARGET NanoRos::NanoRosCpp)
        target_link_libraries(rclcpp::rclcpp INTERFACE NanoRos::NanoRosCpp)
    endif()
endif()
set(rclcpp_FOUND TRUE)
