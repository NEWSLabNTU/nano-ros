cmake_minimum_required(VERSION 3.22)
project(telem_pkg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT COMMAND nano_ros_workspace_pkg_guard)
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: set -DNANO_ROS_ROOT=<path-to-nano-ros> for "
            "standalone builds, or build via the workspace root.")
    endif()
    include("${NANO_ROS_ROOT}/cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Generates the std_msgs C bindings (serialize/deserialize); the auto-link
# sidecar links the generated `std_msgs__nano_ros_c` interface lib.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Low-tier telemetry node (C tiers fixture). Publishes a counter on /telem
# every 100 ms. CALLBACK_GROUPS declares the "telem" group ID; system.toml
# [[node_overrides]] maps it to the [tiers.low] tier (RFC-0015 §4.2).
nano_ros_node_register(
    NAME     telem
    CLASS    telem_pkg::Telem
    LANGUAGE C
    TYPED
    CALLBACK_GROUPS telem
    SOURCES  src/Telem.c)
