cmake_minimum_required(VERSION 3.22)
project(threadx_linux_cpp_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/compat/NrosRclcppCompat.cmake")
find_package(example_interfaces REQUIRED)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nano_ros_node_register(
    NAME    action_server
    CLASS   threadx_linux_cpp_action_server::ActionServer
    SOURCES src/ActionServer.cpp
    DEPLOY  threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_cpp_action_server src/main.c)
target_link_libraries(threadx_cpp_action_server PRIVATE
    threadx_linux_cpp_action_server_action_server_component
    example_interfaces::example_interfaces)
nros_threadx_link_app(threadx_cpp_action_server)
nano_ros_link_rmw(threadx_cpp_action_server RMW "${NROS_RMW}")

# Phase 214 followup — link interface lib so generated headers
# (e.g. example_interfaces.hpp) are on include path for src/*.cpp sources.
target_link_libraries(threadx_linux_cpp_action_server_action_server_component PUBLIC example_interfaces::example_interfaces)
