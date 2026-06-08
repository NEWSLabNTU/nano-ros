cmake_minimum_required(VERSION 3.22)
project(rust_heartbeat_pkg VERSION 0.1.0 LANGUAGES C CXX)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nano_ros_node_register(
    NAME     heartbeat
    CLASS    rust_heartbeat_pkg::Heartbeat
    LANGUAGE RUST
    SOURCES  Cargo.toml
    DEPLOY   native)
