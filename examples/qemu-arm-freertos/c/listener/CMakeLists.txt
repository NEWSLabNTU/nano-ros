cmake_minimum_required(VERSION 3.22)
project(freertos_c_listener VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.5 — consume nano-ros via add_subdirectory(<repo-root>).
# Five levels up from examples/qemu-arm-freertos/<lang>/<rmw>/<example>/.
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/config.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/config.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

add_executable(freertos_c_listener src/main.c)
target_include_directories(freertos_c_listener PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(freertos_c_listener PRIVATE
    std_msgs__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(freertos_c_listener)
nano_ros_link_rmw(freertos_c_listener RMW zenoh)
