cmake_minimum_required(VERSION 3.22)
# Phase 244 D6 — declarative Node pkg; class prefix must equal PROJECT_NAME (L.4).
project(threadx_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.8 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

# Phase 244 D6 — Component pkg shape (mirrors threadx-linux/cpp/talker):
#   1. Register this pkg as a C Node (STATIC component lib).
#   2. Synthesise the ThreadX system_main (this pkg IS its own bringup).
#   3. Build the executable + link the synthesised system + platform.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Message deps resolved from the sibling package.xml SSoT (issue #11).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)
nano_ros_node_register(
    NAME     talker
    CLASS    threadx_c_talker::Talker
    LANGUAGE C
    SOURCES  src/Talker.c
    DEPLOY   threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_c_talker src/main.c)
target_link_libraries(threadx_c_talker PRIVATE
    threadx_c_talker_talker_component
    std_msgs__nano_ros_c)
nros_threadx_link_app(threadx_c_talker)
nano_ros_link_rmw(threadx_c_talker RMW "${NROS_RMW}")
