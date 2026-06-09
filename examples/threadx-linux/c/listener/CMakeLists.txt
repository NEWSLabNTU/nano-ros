cmake_minimum_required(VERSION 3.22)
project(threadx_c_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Message deps resolved from the sibling package.xml SSoT (issue #11).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

add_executable(threadx_c_listener src/main.c)
target_link_libraries(threadx_c_listener PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(threadx_c_listener)
nano_ros_link_rmw(threadx_c_listener RMW "${NROS_RMW}")
