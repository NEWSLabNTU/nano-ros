cmake_minimum_required(VERSION 3.22)
project(cpp_fib_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# phase-263 A4 (actions, C++) — poll-model Fibonacci action client using create_action_client_raw
# + the raw send_goal_async / get_result_async / poll FFI (generated example_interfaces C++ action
# bindings: serialize goal / deserialize result). PRINT-ONLY (one interface pkg, no dedup gap).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_node_register(
    NAME    fib_client
    CLASS   cpp_fib_client_pkg::FibClient
    SOURCES src/FibClient.cpp)

if(TARGET example_interfaces__nano_ros_cpp)
    target_link_libraries(cpp_fib_client_pkg_fib_client_component
        PUBLIC example_interfaces__nano_ros_cpp)
endif()
