# topic_state_monitor synthetic port — Phase 209.G first iteration.
cmake_minimum_required(VERSION 3.22)
project(topic_state_monitor LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW (zenoh|cyclonedds|xrce).")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/compat/NrosRclcppCompat.cmake")

# package.xml is the single source of truth for the msg deps;
# nros_find_interfaces() resolves the transitive closure from its <depend> entries.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Stock ament_cmake_auto-shape below — unchanged from what an Autoware
# CMakeLists would carry.
find_package(ament_cmake_auto REQUIRED)
find_package(rclcpp REQUIRED)
find_package(diagnostic_updater REQUIRED)
ament_auto_find_build_dependencies()

ament_auto_add_executable(topic_state_monitor src/topic_state_monitor.cpp)
ament_target_dependencies(topic_state_monitor rclcpp diagnostic_updater)
target_link_libraries(topic_state_monitor PRIVATE
    std_msgs__nano_ros_cpp
    diagnostic_msgs__nano_ros_cpp)

ament_auto_package()
