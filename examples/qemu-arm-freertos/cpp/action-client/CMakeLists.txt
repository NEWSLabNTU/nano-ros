cmake_minimum_required(VERSION 3.22)
project(freertos_cpp_action_client VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.5 — consume nano-ros via add_subdirectory(<repo-root>).
# Five levels up from examples/qemu-arm-freertos/<lang>/<rmw>/<example>/.
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/nros.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/nros.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

add_executable(freertos_cpp_action_client src/main.cpp)
target_include_directories(freertos_cpp_action_client PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(freertos_cpp_action_client PRIVATE
    example_interfaces::example_interfaces
    NanoRos::NanoRosCpp)
nros_platform_link_app(freertos_cpp_action_client)
nano_ros_link_rmw(freertos_cpp_action_client RMW "${NROS_RMW}")
