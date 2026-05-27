cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_service_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.6 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/nros.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/nros.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

add_executable(nuttx_cpp_service_server src/main.cpp)
target_include_directories(nuttx_cpp_service_server PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(nuttx_cpp_service_server PRIVATE
    example_interfaces__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(nuttx_cpp_service_server)
