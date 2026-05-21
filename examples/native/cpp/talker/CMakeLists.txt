cmake_minimum_required(VERSION 3.22)
project(cpp_talker LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 118.B.3 — collapsed-shape C++ talker. RMW selected at
# cmake configure time via `-DNROS_RMW=<rmw>` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)




add_executable(cpp_talker src/main.cpp)
target_link_libraries(cpp_talker PRIVATE
    std_msgs__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_talker)
