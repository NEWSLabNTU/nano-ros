cmake_minimum_required(VERSION 3.22)
project(freertos_rust_listener_cyclonedds VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 175.B — FreeRTOS Rust CycloneDDS uses the same C startup path
# as the C/C++ examples. CMake owns Cyclone's C++ backend + generated
# descriptors; the Rust crate is imported as a staticlib exposing
# app_main().
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "cyclonedds" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}" CACHE STRING
    "nano-ros root RMW backend" FORCE)
set(NANO_ROS_BOARD mps2-an385-freertos)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")

nros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE C SKIP_INSTALL)
nano_ros_read_config("${CMAKE_CURRENT_SOURCE_DIR}/nros.toml")
nano_ros_generate_config_header(
    "${CMAKE_CURRENT_SOURCE_DIR}/nros.toml"
    "${CMAKE_CURRENT_BINARY_DIR}/include/nros/app_config.h")

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES qemu-freertos-listener
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(freertos_rust_listener_cyclonedds src/cyclonedds_app.c)
target_include_directories(freertos_rust_listener_cyclonedds PRIVATE
    "${CMAKE_CURRENT_BINARY_DIR}/include")
target_link_libraries(freertos_rust_listener_cyclonedds PRIVATE
    qemu_freertos_listener
    std_msgs__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition"
    "-Wl,--no-as-needed,-lstdc++")
nros_platform_link_app(freertos_rust_listener_cyclonedds)
nano_ros_link_rmw(freertos_rust_listener_cyclonedds RMW "${NROS_RMW}")
