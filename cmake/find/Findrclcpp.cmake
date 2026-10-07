# Find module for rclcpp — part of nano-ros's CMake package (phase-482 W1;
# formerly the phase-209 compat stub).
#
# Defines the `rclcpp::rclcpp` IMPORTED INTERFACE target that ROS 2 cmake links
# against (`target_link_libraries(my_target rclcpp::rclcpp)`), forwarding to
# NanoRos::NanoRosCpp. `<rclcpp/rclcpp.hpp>` is an nros-cpp header, so the
# include path comes with that link and this module adds none.
#
# issue 1467 — the umbrella choice has ONE home, and an INTERFACE forward is a
# requirement propagated to a consumer this module cannot see, so it is guarded
# there. An rclcpp-shaped consumer is C++ and will not be a Rust-staticlib
# carrier, so the guard is expected to stay open here; it is applied anyway
# because a per-site exemption is how the rule grows a tenth spelling.
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosRuntimeUmbrella.cmake")
if(NOT TARGET rclcpp::rclcpp)
    add_library(rclcpp::rclcpp INTERFACE IMPORTED)
    nros_link_runtime_umbrella(rclcpp::rclcpp INTERFACE
        CANDIDATES NanoRos::NanoRosCpp)
    # Issue 1239 — ASK for the std surface, exactly as
    # `_nros_ament_apply_target_settings` does per target. The two are meant to
    # be equivalent entry points: that one serves `ament_auto_add_*`, this one a
    # stock `find_package(rclcpp)` + `ament_target_dependencies`, which links
    # this INTERFACE target without calling the per-target helper. INTERFACE,
    # because the flag has to reach whatever links this target.
    target_compile_definitions(rclcpp::rclcpp INTERFACE NROS_CPP_STD=1)
endif()
set(rclcpp_FOUND TRUE)
