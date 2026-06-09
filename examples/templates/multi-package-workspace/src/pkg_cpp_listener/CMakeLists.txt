cmake_minimum_required(VERSION 3.22)
project(pkg_cpp_listener LANGUAGES C CXX)

# Phase 140 — multi-package workspace demo, add_subdirectory shape.
# C++ listener counterpart to pkg_c_talker. Both packages
# add_subdirectory the SAME nano-ros source tree (with the same
# NANO_ROS_PLATFORM / NANO_ROS_RMW cache vars). When built under
# a shared driver (build-all.sh) with NANO_ROS_GEN_CACHE_DIR
# pointing at a workspace-shared scratch dir, the codegen output
# for `std_msgs__nano_ros_cpp` etc. is reused across the C + C++
# packages.

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

add_executable(pkg_cpp_listener src/main.cpp)
target_link_libraries(pkg_cpp_listener PRIVATE
    std_msgs__nano_ros_cpp
    NanoRos::NanoRosCpp)
target_compile_definitions(pkg_cpp_listener PRIVATE NROS_CPP_STD=1)
nros_platform_link_app(pkg_cpp_listener)
