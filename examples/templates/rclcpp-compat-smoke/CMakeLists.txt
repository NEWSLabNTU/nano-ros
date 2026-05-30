# rclcpp-compat smoke — Phase 209 MVP-quartet integration template.
#
# Pure ament_cmake_auto + rclcpp_components shape, mostly verbatim from what a
# ROS 2 user would write. The only nano-ros-specific lines are:
#   1) the platform / rmw selection + `add_subdirectory(<nano-ros>)`,
#   2) the `include(<nano-ros>/cmake/compat/NrosRclcppCompat.cmake)` one-liner,
#   3) the `nros_generate_interfaces(...)` codegen calls (until 209.E ships the
#      bulk `nros generate cpp --workspace` form).
# Everything below those is unmodified `find_package + ament_auto_*`.

cmake_minimum_required(VERSION 3.22)
project(rclcpp_compat_smoke LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1) Pull nano-ros (provides NanoRos::NanoRos, NanoRos::NanoRosCpp + the codegen
#    function). Five levels up: examples/templates/rclcpp-compat-smoke/.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

# 2) Drop-in source-compat for rclcpp / ament_cmake_auto / rclcpp_components /
#    diagnostic_updater. Everything below mirrors a stock ROS 2 CMakeLists.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/compat/NrosRclcppCompat.cmake")

# 3) Generate the C++ bindings for the message packages this node uses. This is
#    nano-ros's current per-package shape; 209.E folds bulk codegen so a ported
#    `find_package(std_msgs)` etc. alone is enough.
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(diagnostic_msgs DEPENDENCIES builtin_interfaces std_msgs LANGUAGE CPP SKIP_INSTALL)

# --- below: unmodified stock-ROS-2 ament_cmake_auto pattern ---

find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
find_package(diagnostic_updater REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(rclcpp_compat_smoke src/talker.cpp)
ament_target_dependencies(rclcpp_compat_smoke rclcpp diagnostic_updater)
target_link_libraries(rclcpp_compat_smoke PRIVATE
    std_msgs__nano_ros_cpp
    diagnostic_msgs__nano_ros_cpp)

ament_auto_package()
