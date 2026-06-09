cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(threadx_linux_cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.8 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Phase 212.L.9 + H.4 — Component pkg shape:
#   1. Register this pkg as a C++ Component (STATIC lib).
#   2. Synthesise the ThreadX system_main (self-bringup: this pkg IS
#      its own bringup; `nros plan` reads system.toml in this dir).
#   3. Build the executable + link the synthesised system + platform.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Message deps resolved from the sibling package.xml SSoT (issue #11) —
# mirrors the native + FreeRTOS C++ shape. `nros_find_interfaces` reads
# package.xml's `<depend>std_msgs</depend>`, generates the C++ bindings,
# and registers `std_msgs__nano_ros_cpp` in this dir's
# NROS_GENERATED_INTERFACE_LIBS so `nano_ros_node_register` auto-links it
# onto the component (Phase 220.G.2).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
nano_ros_node_register(
    NAME    talker
    CLASS   threadx_linux_cpp_talker::Talker
    SOURCES src/Talker.cpp
    DEPLOY  threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_cpp_talker src/main.c)
target_link_libraries(threadx_cpp_talker PRIVATE
    threadx_linux_cpp_talker_talker_component
    std_msgs__nano_ros_cpp)
nros_threadx_link_app(threadx_cpp_talker)
nano_ros_link_rmw(threadx_cpp_talker RMW "${NROS_RMW}")
