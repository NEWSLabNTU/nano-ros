cmake_minimum_required(VERSION 3.22)
project(c_service_server LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 118.B.2 — collapsed-shape C service-server. RMW selected at cmake
# configure time via `-DNROS_RMW=<rmw>` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(example_interfaces DEPENDENCIES builtin_interfaces SKIP_INSTALL)



add_executable(c_service_server src/main.c)
target_link_libraries(c_service_server PRIVATE
    example_interfaces__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_service_server)
