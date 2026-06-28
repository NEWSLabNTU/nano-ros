cmake_minimum_required(VERSION 3.22)
project(c_add_client_pkg VERSION 0.1.0 LANGUAGES C CXX)

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

# phase-263 A1 (services, C projection) — typed component using the generated
# example_interfaces/srv/AddTwoInts C bindings (deserialize request / serialize reply). The
# auto-link sidecar links the generated `example_interfaces__nano_ros_c` interface lib.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME     add_client
    CLASS    c_add_client_pkg::AddClient
    LANGUAGE C
    TYPED
    SOURCES  src/AddClient.c)
